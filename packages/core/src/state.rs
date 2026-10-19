//! `CoreState`: the device catalogs, the user's settings, pairing, and the
//! two sessions (input and exclusive output) that own live audio.

use crate::cd::CdDeck;
use crate::events::{emit_catalog_diff, WsEvent};
use crate::keep_awake::KeepAwake;
use crate::pairing::PairingState;
use crate::pipeline::input::InputWiring;
use crate::pipeline::{InputError, InputSession, LOOPBACK_INPUT};
use crate::sender::airplay_mdns::CatalogDevice;
use crate::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, MockBluetoothAdapter, RecordingPcmSink,
    SystemBluetoothAdapter,
};
use crate::sender::sonos::discovery::{DeviceRegistry, SonosDevice};
use crate::sender::{AudioSender, SenderError};
use crate::session::OutputSession;
use crate::settings::{SavedSettings, SettingsPersistence};
use crate::sync::{OutputKey, SyncController};
use crate::timeline::PcmChunk;
use on_air_api::ChannelMode;
use parking_lot::Mutex as StdMutex;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{
    AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch, Mutex};

pub use crate::session::ActiveOutput;

type BluetoothDeviceCache = Option<(Instant, Vec<BluetoothDevice>)>;

/// Timing tunables; tests shorten them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreConfig {
    /// How long a freshly started core waits before restoring the saved
    /// session, so discovery has a chance to find the speaker first.
    pub restore_delay: Duration,
}

impl Default for CoreConfig {
    fn default() -> Self {
        CoreConfig {
            restore_delay: Duration::from_millis(500),
        }
    }
}

/// Long-running tasks started by `serve_with_state`. Aborting them is the
/// embedding host's choice: the desktop keeps discovery alive while the
/// service is paused so the device lists stay fresh.
#[derive(Default)]
pub struct BackgroundTasks {
    handles: Vec<tokio::task::JoinHandle<()>>,
    mdns: Option<mdns_sd::ServiceDaemon>,
}

impl BackgroundTasks {
    pub fn push(&mut self, handle: tokio::task::JoinHandle<()>) {
        self.handles.push(handle);
    }

    pub fn set_mdns(&mut self, daemon: Option<mdns_sd::ServiceDaemon>) {
        self.mdns = daemon;
    }

    /// Cancel every task and stop the mDNS responder.
    pub fn abort_all(&mut self) {
        for handle in self.handles.drain(..) {
            handle.abort();
        }
        if let Some(mdns) = self.mdns.take() {
            let _ = mdns.shutdown();
        }
    }
}

#[derive(Clone)]
pub struct CoreState {
    pub(crate) patchbay: Arc<crate::patchbay::Patchbay>,
    pub(crate) patchbay_endpoint: bool,
    pub(crate) sender_lock: Arc<Mutex<()>>,
    pub(crate) legacy_output: Option<OutputSession>,
    // Sessions. Each owns its live resources under its own transition lock.
    output: OutputSession,
    input: InputSession,
    /// Serializes configuration changes that span both sessions (an output
    /// switch, an input switch, a sample-rate change, the saved-session
    /// restore) so they apply in a single order.
    pub(crate) config_lock: Arc<Mutex<()>>,
    pub(crate) cd: CdDeck,

    // Buses.
    /// Pipeline PCM: processed interleaved stereo L16 at the input rate,
    /// every chunk stamped with its frame index and capture time on the
    /// shared playback timeline (`crate::timeline`).
    pub audio_tx: broadcast::Sender<PcmChunk>,
    pub ws_tx: broadcast::Sender<WsEvent>,

    /// Cross-protocol sync: groups, latency trims and drift reports.
    /// Shared with patchbay children so a group can span sessions.
    pub sync: Arc<SyncController>,

    /// Acoustic latency calibration (roadmap B10): at most one run at a
    /// time, its progress for `GET /api/sync/calibration`, and (mock mode
    /// only) the CI world the synthetic loopback places behind the probe.
    pub(crate) calibration: Arc<crate::calibration::run::CalibrationController>,

    // Catalogs.
    pub(crate) sonos: Arc<Mutex<DeviceRegistry>>,
    pub(crate) airplay: Arc<StdMutex<Vec<CatalogDevice>>>,
    pub(crate) owntone_base: Arc<StdMutex<String>>,
    pub(crate) bluetooth: Arc<dyn BluetoothAdapter>,
    bluetooth_cache: Arc<Mutex<BluetoothDeviceCache>>,

    /// The isolated states `sync/lifecycle` gives playing sync-group
    /// members, keyed by output: one sender and radio-nonce owner per
    /// member, all reading the shared timeline bus above. Only the root
    /// state holds entries (each child gets an empty registry), so this
    /// never becomes a reference cycle.
    pub(crate) group_children: Arc<tokio::sync::Mutex<HashMap<crate::sync::OutputKey, CoreState>>>,
    /// True in the states [`CoreState::group_child`] hands to playing
    /// sync-group members: each drives its receiver on its own transport
    /// session, like a patchbay endpoint, not through the shared one.
    pub(crate) group_member: bool,

    // Settings.
    pub(crate) eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    /// The DSP quality profile in effect: shared with the input wiring so
    /// each capture's processing task builds its resampler with it.
    pub(crate) dsp_quality: Arc<StdMutex<crate::dsp::DspQuality>>,
    pub(crate) target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub(crate) output_sample_rate_hz: Arc<StdMutex<u32>>,
    output_volume: Arc<AtomicU8>,
    /// Controls whether remote clients may start or configure audio work. The
    /// lightweight HTTP and discovery services remain online while disabled.
    pub service_enabled: Arc<AtomicBool>,
    /// The platform sleep inhibitor, held exactly while the service is on.
    pub keep_awake: Arc<KeepAwake>,
    /// Serializes service on/off so the flag, the saved settings, the last
    /// event clients saw and the inhibitor always agree.
    service_lock: Arc<StdMutex<()>>,
    saved_settings: Arc<StdMutex<SavedSettings>>,
    settings_persistence: Option<SettingsPersistence>,
    settings_revision: Arc<AtomicU64>,
    /// Bumped whenever remembered state actually changes (the service
    /// flag, the volume, the saved input or output...). Observers that
    /// re-read everything anyway — the MPRIS bridge — wait on this instead
    /// of subscribing to [`CoreState::ws_tx`], whose receiver count the
    /// level meter keys off.
    change_signal: watch::Sender<u64>,
    restoring_settings: Arc<AtomicBool>,

    // Pairing.
    pub require_auth: bool,
    pub(crate) pairing: Arc<StdMutex<PairingState>>,
    /// Serialises `/api/pairing/verify` so the PIN check, the on-disk write
    /// (done off the request thread) and the commit stay consistent without
    /// holding `pairing` across the write.
    pub(crate) pairing_verify_lock: Arc<Mutex<()>>,

    // Radio stream health, shared with the Sonos sender's stall recovery.
    pub stream_clients: Arc<AtomicUsize>,
    /// Monotonic heartbeat advanced only when a live stream body is polled.
    /// Sonos recovery uses it to distinguish a connected reader from one that
    /// has stopped draining audio.
    pub stream_progress: Arc<AtomicU64>,
    /// The HTTP port, recorded by `serve_with_state`; receivers pull the
    /// radio stream from it.
    pub(crate) serve_port: Arc<AtomicU16>,
    pub firewall_warning: Arc<StdMutex<Option<on_air_api::FirewallWarning>>>,
    pub(crate) firewall_repair_lock: Arc<Mutex<()>>,
    pub(crate) firewall_repaired_at: Arc<AtomicU64>,
    pub(crate) capture_buffer_ms: Arc<AtomicU32>,
    pub(crate) audio_benchmark_lock: Arc<Mutex<()>>,

    // Mode, config and host plumbing.
    pub(crate) mock: bool,
    pub config: CoreConfig,
    pub(crate) background: Arc<StdMutex<BackgroundTasks>>,
    /// Calls made on mock senders, for tests.
    pub mock_log: Arc<tokio::sync::Mutex<Vec<String>>>,
    /// "This computer" outputs the mock core reports instead of probing
    /// Pulse; empty by default (roadmap A4 tests seed a mono speaker).
    pub mock_local_outputs: Arc<StdMutex<Vec<crate::sender::local::LocalOutput>>>,
    pub(crate) pcm_sink: Arc<RecordingPcmSink>,
}

/// What a request to set the service flag amounts to, given its current value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceTransition {
    Unchanged,
    TurnOn,
    TurnOff,
}

/// Why a settings apply could not happen right now.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("the service is paused")]
    Paused,
    #[error("{0}")]
    Failed(String),
}

pub fn service_transition(current: bool, requested: bool) -> ServiceTransition {
    match (current, requested) {
        (false, true) => ServiceTransition::TurnOn,
        (true, false) => ServiceTransition::TurnOff,
        _ => ServiceTransition::Unchanged,
    }
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;
// Network speakers buffer and read live radio streams in bursts. At 1024 stereo
// frames per chunk this retains roughly 12 seconds / 2 MiB at 44.1 kHz, enough
// to absorb Sonos read pauses without making the audio path unbounded.
const AUDIO_BROADCAST_CAPACITY: usize = 512;

fn bluetooth_ids(devices: &[BluetoothDevice]) -> Vec<(String, String)> {
    devices
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect()
}

/// Whether `name` would capture what this computer plays.
async fn is_system_audio(name: &str) -> bool {
    use crate::pipeline::sources::{self, SYSTEM_AUDIO};
    if name == SYSTEM_AUDIO || name == LOOPBACK_INPUT {
        return true;
    }
    let name = name.to_string();
    tokio::task::spawn_blocking(move || {
        sources::query().is_some_and(|s| s.label_for(&name) == SYSTEM_AUDIO)
    })
    .await
    .unwrap_or(false)
}

impl CoreState {
    pub fn new() -> Self {
        Self::from_saved_settings(SavedSettings::default(), None)
    }

    pub fn new_persistent(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let saved = match crate::settings::load_file(&path) {
            Ok(saved) => saved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SavedSettings::default(),
            Err(error) => {
                eprintln!("could not load on-air settings; using safe defaults: {error}");
                SavedSettings::default()
            }
        };
        let pairing = PairingState::persistent(path.with_file_name("paired-remotes.json"));
        let patchbay_path = path.with_file_name("patchbay.json");
        let mut state = Self::from_saved_settings(saved, Some(SettingsPersistence::new(path)));
        state.patchbay = Arc::new(crate::patchbay::Patchbay::new(Some(patchbay_path), false));
        state.pairing = Arc::new(StdMutex::new(pairing));
        state
    }

    fn from_saved_settings(
        saved_settings: SavedSettings,
        settings_persistence: Option<SettingsPersistence>,
    ) -> Self {
        let (audio_tx, _) = broadcast::channel(AUDIO_BROADCAST_CAPACITY);
        let (ws_tx, _) = broadcast::channel(64);
        let sync = Arc::new(SyncController::new());
        for trim in &saved_settings.sync_trims {
            sync.set_trim_ms(
                &OutputKey::new(&trim.transport, &trim.device_id),
                &trim.transport,
                trim.trim_ms,
            );
        }
        for measurement in &saved_settings.sync_calibrations {
            sync.set_calibration(measurement);
        }
        for group in &saved_settings.sync_groups {
            sync.restore_group(group);
        }
        let (change_signal, _) = watch::channel(0u64);
        let cd = CdDeck::new();
        cd.add_event_sink(ws_tx.clone());
        let eq_gains_db = Arc::new(StdMutex::new(saved_settings.eq_gains_db));
        let dsp_quality = Arc::new(StdMutex::new(
            saved_settings.dsp_quality.unwrap_or_default(),
        ));
        let target_sample_rate_hz = Arc::new(StdMutex::new(saved_settings.input_sample_rate_hz));
        let output_sample_rate_hz = Arc::new(StdMutex::new(saved_settings.output_sample_rate_hz));
        let output = OutputSession::new(ws_tx.clone(), output_sample_rate_hz.clone());
        let capture_buffer_ms = Arc::new(AtomicU32::new(saved_settings.capture_buffer_ms));
        let input = InputSession::new(InputWiring {
            audio_tx: audio_tx.clone(),
            ws_tx: ws_tx.clone(),
            eq_gains_db: eq_gains_db.clone(),
            input_rate_hz: target_sample_rate_hz.clone(),
            cd: cd.clone(),
            capture_buffer_ms: capture_buffer_ms.clone(),
            dsp_quality: dsp_quality.clone(),
        });
        CoreState {
            patchbay: Arc::new(crate::patchbay::Patchbay::new(None, false)),
            patchbay_endpoint: false,
            sender_lock: Arc::new(Mutex::new(())),
            legacy_output: None,
            output,
            input,
            config_lock: Arc::new(Mutex::new(())),
            cd,
            audio_tx,
            ws_tx,
            sync,
            calibration: Arc::new(crate::calibration::run::CalibrationController::new()),
            sonos: Arc::new(Mutex::new(DeviceRegistry::new())),
            airplay: Arc::new(StdMutex::new(Vec::new())),
            owntone_base: Arc::new(StdMutex::new("http://127.0.0.1:3689".into())),
            bluetooth: Arc::new(SystemBluetoothAdapter::from_host()),
            bluetooth_cache: Arc::new(Mutex::new(None)),
            group_children: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            group_member: false,
            eq_gains_db,
            dsp_quality,
            target_sample_rate_hz,
            output_sample_rate_hz,
            output_volume: Arc::new(AtomicU8::new(saved_settings.volume)),
            service_enabled: Arc::new(AtomicBool::new(saved_settings.service_enabled)),
            keep_awake: Arc::new(KeepAwake::system(saved_settings.service_enabled)),
            service_lock: Arc::new(StdMutex::new(())),
            saved_settings: Arc::new(StdMutex::new(saved_settings)),
            settings_persistence,
            settings_revision: Arc::new(AtomicU64::new(0)),
            change_signal,
            restoring_settings: Arc::new(AtomicBool::new(false)),
            require_auth: false,
            pairing: Arc::new(StdMutex::new(PairingState::new())),
            pairing_verify_lock: Arc::new(Mutex::new(())),
            stream_clients: Arc::new(AtomicUsize::new(0)),
            stream_progress: Arc::new(AtomicU64::new(0)),
            serve_port: Arc::new(AtomicU16::new(crate::DEFAULT_PORT)),
            firewall_warning: Arc::new(StdMutex::new(None)),
            firewall_repair_lock: Arc::new(Mutex::new(())),
            firewall_repaired_at: Arc::new(AtomicU64::new(0)),
            capture_buffer_ms,
            audio_benchmark_lock: Arc::new(Mutex::new(())),
            mock: false,
            config: CoreConfig::default(),
            background: Arc::new(StdMutex::new(BackgroundTasks::default())),
            mock_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            mock_local_outputs: Arc::new(StdMutex::new(Vec::new())),
            pcm_sink: Arc::new(RecordingPcmSink::default()),
        }
    }

    /// Replace the sleep inhibitor, for tests and for hosts that hold their
    /// own. The replacement is held or released to match the service flag
    /// the moment it is installed.
    pub fn with_keep_awake(mut self, keep_awake: KeepAwake) -> Self {
        keep_awake.set_enabled(self.service_enabled.load(Ordering::Acquire));
        self.keep_awake = Arc::new(keep_awake);
        self
    }

    /// Whether a platform sleep inhibitor is held right now.
    pub fn sleep_inhibited(&self) -> bool {
        self.keep_awake.is_holding()
    }

    /// E2E / CI: fake inputs and transports, fixed pairing PIN, no hardware.
    pub async fn new_mock() -> Self {
        // The mock core must never start a real `systemd-inhibit` on CI.
        let mut state = Self::new().with_keep_awake(KeepAwake::with_spawner(
            None,
            Box::new(crate::keep_awake::NoopSpawner),
            true,
        ));
        state.set_mock(true);
        state.seed_mock_hardware().await;
        state
    }

    /// E2E / CI: a mock core that also persists settings to `path`, so a
    /// test can restart it and assert what was saved survived.
    pub async fn new_mock_persistent(path: impl Into<PathBuf>) -> Self {
        let mut state = Self::new_persistent(path).with_keep_awake(KeepAwake::with_spawner(
            None,
            Box::new(crate::keep_awake::NoopSpawner),
            true,
        ));
        state.set_mock(true);
        state.seed_mock_hardware().await;
        state
    }

    /// The fake hardware a mock core pretends to have: one of each
    /// transport the product supports.
    async fn seed_mock_hardware(&mut self) {
        self.pairing = Arc::new(StdMutex::new(PairingState::mock()));
        self.input.set_mock_inputs(vec!["Mock Monitor".into()]);
        *self.airplay.lock() = vec![CatalogDevice {
            id: "ap-living".into(),
            name: "Living Room AirPlay".into(),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: String::new(),
        }];
        self.bluetooth = Arc::new(MockBluetoothAdapter::with_devices(vec![
            BluetoothDevice {
                id: "bt-speaker".into(),
                name: "Mock Bluetooth Speaker".into(),
                paired: true,
                connected: false,
                audio_endpoint: Some("bt-speaker".into()),
            },
            BluetoothDevice {
                id: "bt-unpaired".into(),
                name: "New Bluetooth Speaker".into(),
                paired: false,
                connected: false,
                audio_endpoint: None,
            },
        ]));
        let sonos = SonosDevice::discovered(
            "uuid:mock-sonos",
            "http://127.0.0.1:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "Mock Sonos",
        );
        self.sonos.lock().await.upsert(sonos, Instant::now());
    }

    /// Hardware-free mode: senders are fakes, inputs come from the mock
    /// list, and "This computer" outputs come from
    /// [`Self::mock_local_outputs`] instead of probing Pulse.
    pub fn set_mock_local_outputs(&self, outputs: Vec<crate::sender::local::LocalOutput>) {
        *self.mock_local_outputs.lock() = outputs;
    }

    /// Hardware-free mode: senders are fakes, inputs come from the mock
    /// list, and the laptop speakers are never touched.
    pub fn set_mock(&mut self, mock: bool) {
        self.mock = mock;
        self.patchbay = Arc::new(crate::patchbay::Patchbay::new(None, mock));
        self.output.set_manage_local_sink(!mock);
        self.input.set_mock(mock);
    }

    // ---- sessions -------------------------------------------------------

    /// Isolated PCM, sender, nonce and stream health for a virtual network
    /// sink. Discovery and the service flag are shared with the parent; the
    /// sync registry is too, so a sync group can span root and patchbay
    /// sessions.
    pub(crate) fn patchbay_child(&self) -> Self {
        let mut child = self.clone();
        child.patchbay_endpoint = true;
        child.legacy_output = Some(self.output.clone());
        let (audio_tx, _) = broadcast::channel(AUDIO_BROADCAST_CAPACITY);
        let (ws_tx, _) = broadcast::channel(64);
        let (change_signal, _) = watch::channel(0u64);
        child.audio_tx = audio_tx;
        child.ws_tx = ws_tx;
        child.change_signal = change_signal;
        child.patchbay = Arc::new(crate::patchbay::Patchbay::new(None, self.mock));
        child.settings_persistence = None;
        child.saved_settings = Arc::new(StdMutex::new(SavedSettings::default()));
        child.settings_revision = Arc::new(AtomicU64::new(0));
        child.config_lock = Arc::new(Mutex::new(()));
        child.eq_gains_db = Arc::new(StdMutex::new([0.0; 5]));
        child.dsp_quality = Arc::new(StdMutex::new(crate::dsp::DspQuality::default()));
        child.target_sample_rate_hz = Arc::new(StdMutex::new(44100));
        child.output_sample_rate_hz = Arc::new(StdMutex::new(44100));
        child.output_volume = Arc::new(AtomicU8::new(self.output_volume()));
        child.stream_clients = Arc::new(AtomicUsize::new(0));
        child.stream_progress = Arc::new(AtomicU64::new(0));
        child.background = Arc::new(StdMutex::new(BackgroundTasks::default()));
        child.cd = CdDeck::new();
        child.output = OutputSession::new(child.ws_tx.clone(), child.output_sample_rate_hz.clone());
        child.output.set_manage_local_sink(false);
        child.input = InputSession::new(InputWiring {
            audio_tx: child.audio_tx.clone(),
            ws_tx: child.ws_tx.clone(),
            eq_gains_db: child.eq_gains_db.clone(),
            input_rate_hz: child.target_sample_rate_hz.clone(),
            cd: child.cd.clone(),
            // Patchbay captures follow the user's capture buffer too.
            capture_buffer_ms: child.capture_buffer_ms.clone(),
            dsp_quality: child.dsp_quality.clone(),
        });
        child.input.set_mock(self.mock);
        child
    }

    /// The exclusive-output session.
    pub fn output(&self) -> &OutputSession {
        &self.output
    }

    /// The live-input session.
    pub fn input(&self) -> &InputSession {
        &self.input
    }

    /// Follow the change notice: each tick means remembered state changed
    /// and a full re-read is worthwhile.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_signal.subscribe()
    }

    /// Identity of the output that owns exclusivity (starting, live or
    /// failed).
    pub fn active_output(&self) -> Option<ActiveOutput> {
        self.output.active()
    }

    /// Activate an input and remember it. The caller holds `config_lock`.
    pub(crate) async fn activate_input_locked(&self, name: &str) -> Result<(), InputError> {
        if self
            .active_output()
            .is_some_and(|output| output.transport == crate::sender::local::TRANSPORT)
            && is_system_audio(name).await
        {
            return Err(InputError::Feedback(crate::session::FEEDBACK_LOOP));
        }
        let resolved = self.input.activate(name).await?;
        self.remember_input(resolved);
        Ok(())
    }

    /// Thin wrapper over [`OutputSession::activate`] for a sender without a
    /// catalog identity.
    pub async fn activate_sender(
        &self,
        new_sender: Box<dyn AudioSender>,
    ) -> Result<(), SenderError> {
        self.output.activate(new_sender, None, None).await
    }

    /// Activate a sender that does not pull the radio stream (or a test
    /// double). Wraps [`OutputSession::activate`].
    pub async fn activate_sender_as(
        &self,
        new_sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
    ) -> Result<(), SenderError> {
        self.output.activate(new_sender, identity, None).await
    }

    /// Exclusive-output switch with a radio nonce. Wraps
    /// [`OutputSession::activate`].
    pub async fn activate_sender_streaming(
        &self,
        new_sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
        stream_nonce: Option<String>,
    ) -> Result<(), SenderError> {
        self.output
            .activate(new_sender, identity, stream_nonce)
            .await
    }

    /// Wraps [`OutputSession::deactivate`].
    pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
        self.output.deactivate().await
    }

    // ---- settings -------------------------------------------------------

    pub fn eq_gains_db(&self) -> [f32; 5] {
        *self.eq_gains_db.lock()
    }

    /// The DSP quality profile in effect: the user's explicit choice, or
    /// [`crate::dsp::DspQuality::default`] while they have not chosen one.
    pub fn dsp_quality(&self) -> crate::dsp::DspQuality {
        *self.dsp_quality.lock()
    }

    /// `GET /api/audio/quality`'s view: the chosen profile, the last
    /// benchmark's verdict (the current profile until one has run), and
    /// every selectable profile.
    pub fn audio_quality(&self) -> on_air_api::AudioQualityResponse {
        let quality = self.dsp_quality();
        let recommended = self
            .saved_settings
            .lock()
            .recommended_dsp_quality
            .unwrap_or(quality);
        on_air_api::AudioQualityResponse {
            quality,
            recommended,
            profiles: crate::dsp::DspQuality::ALL.to_vec(),
        }
    }

    /// The pipeline (input) sample rate.
    pub fn input_sample_rate_hz(&self) -> u32 {
        *self.target_sample_rate_hz.lock()
    }

    /// The requested or negotiated output sample rate.
    pub fn output_sample_rate_hz(&self) -> u32 {
        *self.output_sample_rate_hz.lock()
    }

    /// The radio stream URL receivers pull for `nonce`, on the port this
    /// core is served on.
    pub fn stream_url(&self, lan_ip: IpAddr, nonce: &str) -> String {
        crate::session::stream_url(lan_ip, self.serve_port.load(Ordering::Acquire), nonce)
    }

    pub fn output_volume(&self) -> u8 {
        self.output_volume.load(Ordering::Acquire)
    }

    pub fn pairing(&self) -> parking_lot::MutexGuard<'_, PairingState> {
        self.pairing.lock()
    }

    fn update_saved_settings(&self, update: impl FnOnce(&mut SavedSettings)) {
        let mut saved = self.saved_settings.lock();
        let previous = saved.clone();
        update(&mut saved);
        if *saved == previous {
            return;
        }
        self.settings_revision.fetch_add(1, Ordering::AcqRel);
        self.change_signal.send_modify(|stamp| *stamp += 1);
        if let Some(persistence) = self.settings_persistence.as_ref() {
            persistence.queue(saved.clone());
        }
    }

    pub(crate) fn remember_input(&self, name: String) {
        self.update_saved_settings(|saved| saved.active_input = Some(name));
    }

    pub(crate) fn clear_saved_input(&self) {
        self.update_saved_settings(|saved| saved.active_input = None);
    }

    pub(crate) fn remember_output(&self, output: ActiveOutput) {
        self.update_saved_settings(|saved| saved.active_output = Some(output));
    }

    pub(crate) fn clear_saved_output(&self) {
        self.update_saved_settings(|saved| saved.active_output = None);
    }

    pub(crate) fn remember_volume(&self, volume: u8) {
        self.output_volume.store(volume, Ordering::Release);
        self.update_saved_settings(|saved| saved.volume = volume);
    }

    /// One output's saved channel mode (roadmap A4). Absent settings mean
    /// [`ChannelMode::Auto`], the historical fit.
    pub fn channel_mode(&self, transport: &str, device_id: &str) -> ChannelMode {
        self.saved_settings
            .lock()
            .channel_modes
            .iter()
            .find(|saved| saved.transport == transport && saved.device_id == device_id)
            .map(|saved| saved.mode)
            .unwrap_or_default()
    }

    /// Persist one output's channel mode (`channel_modes` in settings.json).
    /// `auto` is stored as "absent": it is the default, and dropping the
    /// entry keeps old cores reading the file unchanged.
    pub(crate) fn remember_channel_mode(
        &self,
        transport: &str,
        device_id: &str,
        mode: ChannelMode,
    ) {
        self.update_saved_settings(|saved| {
            saved
                .channel_modes
                .retain(|entry| !(entry.transport == transport && entry.device_id == device_id));
            if mode != ChannelMode::Auto {
                saved.channel_modes.push(crate::settings::SavedChannelMode {
                    transport: transport.to_string(),
                    device_id: device_id.to_string(),
                    mode,
                });
            }
        });
    }

    pub(crate) fn remember_capture_buffer(&self, buffer_ms: u32) {
        self.update_saved_settings(|saved| saved.capture_buffer_ms = buffer_ms);
    }

    /// Isolated sender, nonce and stream counters for one playing
    /// sync-group member. Unlike a patchbay child this shares the ROOT's
    /// timeline bus (`audio_tx`), input rate and sync registry — the members
    /// of a group must read one timeline to hold one presentation epoch —
    /// but owns its output session and output rate so a mixed-rate group
    /// never fights over one mutex. Discovery, the service flag and the
    /// mock log stay shared.
    pub(crate) fn group_child(&self) -> Self {
        let mut child = self.clone();
        // Each child gets its own empty registry (and sender/config locks),
        // so this never becomes a reference cycle and concurrent member
        // starts never serialize behind one switch lock.
        child.group_children = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        child.sender_lock = Arc::new(Mutex::new(()));
        child.config_lock = Arc::new(Mutex::new(()));
        child.settings_persistence = None;
        child.saved_settings = Arc::new(StdMutex::new(SavedSettings::default()));
        child.settings_revision = Arc::new(AtomicU64::new(0));
        child.output_sample_rate_hz = Arc::new(StdMutex::new(*self.output_sample_rate_hz.lock()));
        child.stream_clients = Arc::new(AtomicUsize::new(0));
        child.stream_progress = Arc::new(AtomicU64::new(0));
        let (ws_tx, _) = broadcast::channel(64);
        child.ws_tx = ws_tx;
        child.output = OutputSession::new(child.ws_tx.clone(), child.output_sample_rate_hz.clone());
        child.output.set_manage_local_sink(!self.mock);
        child.group_member = true;
        child
    }

    /// The playing sync-group member whose radio stream owns `nonce`, if
    /// any.
    pub async fn group_stream_state(&self, nonce: &str) -> Option<CoreState> {
        self.group_children
            .lock()
            .await
            .values()
            .find(|child| child.output().snapshot().serves_radio(nonce))
            .cloned()
    }

    /// Persist the sync groups exactly as configured (`sync_groups` in
    /// settings.json). They are restored created and stopped.
    pub(crate) fn remember_sync_groups(&self) {
        let groups = self.sync.saved_groups();
        self.update_saved_settings(|saved| saved.sync_groups = groups.clone());
    }

    // ---- first-run tuning and DSP quality ------------------------------

    /// A snapshot of the saved settings for the status surfaces.
    pub fn saved_settings(&self) -> SavedSettings {
        self.saved_settings.lock().clone()
    }

    /// Whether automatic first-run tuning is still pending here (`None` in
    /// [`SavedSettings::recommended_dsp_quality`]).
    pub fn first_run_tuning_pending(&self) -> bool {
        self.saved_settings.lock().recommended_dsp_quality.is_none()
    }

    /// Record that an audio check completed on this machine (the manual
    /// button included): the first-run trigger flips to "tuned".
    /// Non-destructive — no audio setting changes.
    pub fn record_check_verdict(&self, quality: crate::dsp::DspQuality) {
        self.update_saved_settings(|saved| saved.recommended_dsp_quality = Some(quality));
    }

    /// Apply and record the automatic first-run tuning verdict (see
    /// [`crate::tuning`]): settings the user has chosen are never
    /// overwritten, and the whole verdict is recorded in
    /// [`SavedSettings::first_run_tuning`]. One capture restart makes the
    /// applied settings take effect; if it fails, everything is restored
    /// and nothing is recorded, so a later run retries.
    pub(crate) async fn record_first_run_tuning(
        &self,
        outcome: &crate::audio_benchmark::BenchmarkOutcome,
    ) -> Result<crate::tuning::AppliedTuning, ApplyError> {
        let _configuration = self.config_lock.lock().await;
        if !self.service_enabled.load(Ordering::Acquire) {
            return Err(ApplyError::Paused);
        }
        let previous = self.saved_settings.lock().clone();
        let mut applied = crate::tuning::AppliedTuning::default();
        self.update_saved_settings(|saved| {
            applied = crate::tuning::apply_verdict(
                saved,
                outcome.report.recommended_buffer_ms,
                outcome.recommended_quality,
                crate::tuning::now_unix_s(),
            );
        });
        *self.dsp_quality.lock() = self.saved_settings.lock().dsp_quality.unwrap_or_default();
        // The quality profile only shapes a resampler when the rates
        // differ; no resampler, no restart for it.
        let quality_needs_restart =
            applied.quality && self.input_sample_rate_hz() != self.output_sample_rate_hz();
        if applied.buffer || quality_needs_restart {
            if let Err(error) = self.input().restart().await {
                self.update_saved_settings(|saved| *saved = previous.clone());
                *self.dsp_quality.lock() = previous.dsp_quality.unwrap_or_default();
                return Err(ApplyError::Failed(format!(
                    "could not apply the first-run tuning: {error}"
                )));
            }
        }
        Ok(applied)
    }

    /// Set the capture buffer and restart capture so it takes effect. The
    /// shared path behind `PUT /api/audio/buffer` and the first-run tuning.
    pub(crate) async fn apply_capture_buffer(&self, buffer_ms: u32) -> Result<(), ApplyError> {
        let _configuration = self.config_lock.lock().await;
        if !self.service_enabled.load(Ordering::Acquire) {
            return Err(ApplyError::Paused);
        }
        let previous = self.capture_buffer_ms.swap(buffer_ms, Ordering::AcqRel);
        if previous == buffer_ms {
            return Ok(());
        }
        if let Err(error) = self.input().restart().await {
            self.capture_buffer_ms.store(previous, Ordering::Release);
            let restore = self
                .input()
                .restart()
                .await
                .err()
                .map(|e| format!("; restoring capture also failed: {e}"))
                .unwrap_or_default();
            return Err(ApplyError::Failed(format!(
                "could not apply capture buffer: {error}{restore}"
            )));
        }
        self.remember_capture_buffer(buffer_ms);
        Ok(())
    }

    /// Record the user's explicit DSP quality choice and restart capture
    /// when the effective profile changes, so the new resampler takes
    /// effect. The path behind `PUT /api/audio/quality`; a choice always
    /// wins over the first-run tuning's recommendation.
    pub(crate) async fn apply_dsp_quality(
        &self,
        quality: crate::dsp::DspQuality,
    ) -> Result<(), ApplyError> {
        let _configuration = self.config_lock.lock().await;
        if !self.service_enabled.load(Ordering::Acquire) {
            return Err(ApplyError::Paused);
        }
        let previous = self.saved_settings.lock().dsp_quality;
        if previous == Some(quality) {
            return Ok(());
        }
        let effective_before = previous.unwrap_or_default();
        self.update_saved_settings(|saved| saved.dsp_quality = Some(quality));
        *self.dsp_quality.lock() = quality;
        let resampler_live = self.input_sample_rate_hz() != self.output_sample_rate_hz();
        if effective_before != quality && resampler_live {
            if let Err(error) = self.input().restart().await {
                self.update_saved_settings(|saved| saved.dsp_quality = previous);
                *self.dsp_quality.lock() = effective_before;
                return Err(ApplyError::Failed(format!(
                    "could not apply DSP quality: {error}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn remember_eq(&self, gains: [f32; 5]) {
        self.update_saved_settings(|saved| saved.eq_gains_db = gains);
    }

    /// Persist one output's latency trim (`sync_trims` in settings.json).
    pub(crate) fn remember_sync_trim(&self, transport: &str, device_id: &str, trim_ms: i32) {
        self.update_saved_settings(|saved| {
            match saved
                .sync_trims
                .iter_mut()
                .find(|trim| trim.transport == transport && trim.device_id == device_id)
            {
                Some(existing) => existing.trim_ms = trim_ms,
                None => saved.sync_trims.push(crate::settings::SavedSyncTrim {
                    transport: transport.to_string(),
                    device_id: device_id.to_string(),
                    trim_ms,
                }),
            }
        });
    }

    /// Apply a completed acoustic measurement (roadmap B10): it becomes
    /// the latency the sync machinery uses instead of the documented
    /// default (`calibrated` turns true) and is persisted (`sync_calibrations`
    /// in settings.json).
    pub(crate) fn apply_calibration(&self, measurement: &on_air_api::CalibrationMeasurement) {
        self.sync.set_calibration(measurement);
        self.update_saved_settings(|saved| {
            match saved.sync_calibrations.iter_mut().find(|saved| {
                saved.transport == measurement.transport && saved.device_id == measurement.device_id
            }) {
                Some(existing) => *existing = measurement.clone(),
                None => saved.sync_calibrations.push(measurement.clone()),
            }
        });
    }

    pub(crate) fn remember_sample_rates(&self) {
        let input = *self.target_sample_rate_hz.lock();
        let output = *self.output_sample_rate_hz.lock();
        self.update_saved_settings(|saved| {
            saved.input_sample_rate_hz = input;
            saved.output_sample_rate_hz = output;
        });
    }

    pub fn set_service_enabled(&self, enabled: bool) {
        self.service_enabled.store(enabled, Ordering::Release);
        self.update_saved_settings(|saved| saved.service_enabled = enabled);
    }

    /// The synchronous half of a service change: claim the flag, persist it,
    /// tell WebSocket clients, then hold or release the sleep inhibitor. The
    /// whole step runs under one lock so concurrent on and off requests
    /// apply in a single order.
    pub fn begin_service_transition(&self, enabled: bool) -> ServiceTransition {
        let _serial = self.service_lock.lock();
        if self
            .service_enabled
            .compare_exchange(!enabled, enabled, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return ServiceTransition::Unchanged;
        }
        self.update_saved_settings(|saved| saved.service_enabled = enabled);
        let _ = self.ws_tx.send(WsEvent::ServiceStateChanged { enabled });
        self.keep_awake.set_enabled(enabled);
        service_transition(!enabled, enabled)
    }

    /// The asynchronous half: stop the live audio work after turning off,
    /// or restore the saved session after turning on.
    pub async fn finish_service_transition(&self, transition: ServiceTransition) {
        match transition {
            ServiceTransition::Unchanged => {}
            ServiceTransition::TurnOff => {
                self.shutdown().await;
                // Turned back on while audio was stopping: bring it back.
                if self.service_enabled.load(Ordering::Acquire) {
                    self.spawn_saved_session_restore();
                }
            }
            ServiceTransition::TurnOn => {
                self.spawn_saved_session_restore();
            }
        }
    }

    fn persist_settings_now(&self) {
        let Some(persistence) = self.settings_persistence.as_ref() else {
            return;
        };
        let saved = self.saved_settings.lock().clone();
        if let Err(error) = persistence.save_now(&saved) {
            eprintln!("could not flush on-air settings: {error}");
        }
    }

    pub fn spawn_saved_session_restore(&self) -> Option<tokio::task::JoinHandle<()>> {
        // Snapshot and revision are read under one lock so a change that
        // lands between them cannot make the restore think it is current.
        let (saved, revision) = {
            let saved = self.saved_settings.lock();
            (
                saved.clone(),
                self.settings_revision.load(Ordering::Acquire),
            )
        };
        if self.settings_persistence.is_none()
            || !self.service_enabled.load(Ordering::Acquire)
            || (saved.active_input.is_none() && saved.active_output.is_none())
            || self
                .restoring_settings
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return None;
        }
        let state = self.clone();
        Some(tokio::spawn(async move {
            state.restore_saved_session(saved, revision).await;
            state.restoring_settings.store(false, Ordering::Release);
        }))
    }

    async fn restore_saved_session(&self, saved: SavedSettings, revision: u64) {
        tokio::time::sleep(self.config.restore_delay).await;

        let still_current = || {
            self.service_enabled.load(Ordering::Acquire)
                && self.settings_revision.load(Ordering::Acquire) == revision
        };

        if let Some(input) = saved.active_input.as_deref() {
            let mut last_error = None;
            for _ in 0..10 {
                if !still_current() {
                    return;
                }
                let result = {
                    let _configuration = self.config_lock.lock().await;
                    // A user change may have landed while we waited for the
                    // lock; it wins over the restore.
                    if !still_current() {
                        return;
                    }
                    self.activate_input_locked(input).await
                };
                match result {
                    Ok(()) => {
                        last_error = None;
                        break;
                    }
                    Err(error) => last_error = Some(error),
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            if let Some(error) = last_error {
                eprintln!("could not restore saved input {input:?}: {error}");
                // Inputs saved before the curated list (raw ALSA names) are
                // gone for good; carry on with what the computer plays.
                if matches!(error, InputError::NotFound(_)) && input != LOOPBACK_INPUT {
                    let _configuration = self.config_lock.lock().await;
                    if still_current() {
                        if let Err(error) = self.activate_input_locked(LOOPBACK_INPUT).await {
                            eprintln!("could not fall back to system audio: {error}");
                        }
                    }
                }
            }
        }

        let Some(output) = saved.active_output else {
            return;
        };
        let mut last_error = None;
        for _ in 0..20 {
            if !still_current() {
                return;
            }
            let result = {
                let _configuration = self.config_lock.lock().await;
                if !still_current() {
                    return;
                }
                crate::session::activate(self, &output.transport, &output.device_id).await
            };
            match result {
                Ok(()) => return,
                Err(
                    error @ (crate::session::ActivateError::UnknownTransport(_)
                    | crate::session::ActivateError::Unsupported(_)),
                ) => {
                    eprintln!(
                        "could not restore saved {} output {:?}: {error}",
                        output.transport, output.device_name
                    );
                    return;
                }
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        if let Some(error) = last_error {
            eprintln!(
                "could not restore saved {} output {:?}: {error}",
                output.transport, output.device_name
            );
        }
    }

    // ---- catalogs -------------------------------------------------------

    /// Start the Sonos and AirPlay finders.
    pub(crate) fn spawn_discovery(&self) -> [tokio::task::JoinHandle<()>; 2] {
        [
            crate::sender::sonos::discovery::spawn(self.sonos.clone(), self.ws_tx.clone()),
            crate::sender::airplay_mdns::spawn(
                self.airplay.clone(),
                self.owntone_base.clone(),
                self.ws_tx.clone(),
            ),
        ]
    }

    /// Cache synchronous OS Bluetooth enumeration briefly. This prevents the
    /// desktop and phone refresh loops from launching duplicate `pactl`/CPAL
    /// probes while still making newly connected devices appear promptly.
    pub async fn bluetooth_devices(&self) -> Result<Vec<BluetoothDevice>, String> {
        const CACHE_TTL: Duration = Duration::from_secs(15);
        let mut cache = self.bluetooth_cache.lock().await;
        if let Some((updated, devices)) = cache.as_ref() {
            if updated.elapsed() < CACHE_TTL {
                return Ok(devices.clone());
            }
        }
        let adapter = self.bluetooth.clone();
        let devices = tokio::task::spawn_blocking(move || adapter.list())
            .await
            .map_err(|error| format!("Bluetooth discovery task failed: {error}"))?;
        if let Some((_, previous)) = cache.as_ref() {
            emit_catalog_diff(
                &self.ws_tx,
                "bluetooth",
                &bluetooth_ids(previous),
                &bluetooth_ids(&devices),
            );
        }
        *cache = Some((Instant::now(), devices.clone()));
        Ok(devices)
    }

    pub async fn invalidate_bluetooth_cache(&self) {
        *self.bluetooth_cache.lock().await = None;
    }

    // ---- lifecycle ------------------------------------------------------

    /// Stop all live audio resources before the host process exits or the
    /// service is paused. Discovery and mDNS keep running so device lists
    /// stay fresh while paused; use [`abort_background_tasks`] to end them.
    ///
    /// [`abort_background_tasks`]: Self::abort_background_tasks
    pub async fn shutdown(&self) {
        self.patchbay.stop().await;
        let state = self.clone();
        let _ = tokio::task::spawn_blocking(move || state.persist_settings_now()).await;
        let _configuration = self.config_lock.lock().await;
        if let Err(error) = self.output.deactivate().await {
            eprintln!("could not stop active output cleanly; forcing local shutdown: {error}");
            self.output.abandon().await;
        }
        self.input.stop().await;
        self.output.restore_local_sink().await;
    }

    /// Cancel discovery, CD watching and the mDNS advertisement.
    pub fn abort_background_tasks(&self) {
        self.background.lock().abort_all();
    }
}

impl Default for CoreState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn concurrent_service_toggles_leave_flag_settings_and_inhibitor_agreeing() {
        for round in 0..300 {
            let (keep_awake, _spawner) = crate::keep_awake::testing::recording(true);
            let state = CoreState::new().with_keep_awake(keep_awake);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let (a, b) = (state.clone(), state.clone());
            let (ba, bb) = (barrier.clone(), barrier.clone());
            let off = std::thread::spawn(move || {
                ba.wait();
                a.begin_service_transition(false);
            });
            let on = std::thread::spawn(move || {
                bb.wait();
                b.begin_service_transition(true);
            });
            off.join().unwrap();
            on.join().unwrap();
            let live = state.service_enabled.load(Ordering::Acquire);
            assert_eq!(
                state.saved_settings.lock().service_enabled,
                live,
                "round {round}: saved flag disagrees with the live flag"
            );
            assert_eq!(
                state.keep_awake.is_enabled(),
                live,
                "round {round}: inhibitor disagrees with the live flag"
            );
        }
    }

    #[tokio::test]
    async fn an_activation_queued_behind_a_switch_is_refused_once_the_service_is_off() {
        let state = CoreState::new_mock().await;
        let app = crate::build_router(state.clone());
        let held = state.config_lock.clone().lock_owned().await;
        let request = tokio::spawn({
            use tower::ServiceExt;
            let app = app.clone();
            async move {
                app.oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/outputs/active")
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(
                            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            }
        });
        // Let the request pass the service gate and queue on the lock.
        tokio::time::sleep(Duration::from_millis(100)).await;
        state.begin_service_transition(false);
        drop(held);
        assert_eq!(
            request.await.unwrap(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(state.active_output().is_none());
    }

    #[test]
    fn a_new_state_holds_the_inhibitor_only_while_the_service_is_on() {
        let (keep_awake, spawner) = crate::keep_awake::testing::recording(true);
        let state = CoreState::new().with_keep_awake(keep_awake);
        assert!(state.sleep_inhibited());
        assert_eq!(spawner.events().len(), 1);
    }

    fn settings_test_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "on-air-state-{name}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    fn mock_sonos() -> SonosDevice {
        SonosDevice::discovered(
            "uuid:mock-sonos",
            "http://127.0.0.1:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "Mock Sonos",
        )
    }

    #[tokio::test]
    async fn persistent_state_restores_the_last_complete_mixer_session() {
        let path = settings_test_path("restore");
        let initial = CoreState::new_persistent(path.clone());
        initial.remember_input("Mock Monitor".into());
        initial.remember_output(ActiveOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into(),
            device_name: "Mock Sonos".into(),
        });
        initial.remember_volume(37);
        *initial.eq_gains_db.lock() = [1.0, 2.0, 3.0, 4.0, 5.0];
        initial.remember_eq([1.0, 2.0, 3.0, 4.0, 5.0]);
        *initial.target_sample_rate_hz.lock() = 48_000;
        *initial.output_sample_rate_hz.lock() = 44_100;
        initial.remember_sample_rates();
        initial.persist_settings_now();
        tokio::time::sleep(Duration::from_millis(250)).await;
        drop(initial);

        let mut restored = CoreState::new_persistent(path.clone());
        restored.set_mock(true);
        restored.config.restore_delay = Duration::from_millis(1);
        restored
            .input()
            .set_mock_inputs(vec!["Mock Monitor".into()]);
        restored
            .sonos
            .lock()
            .await
            .upsert(mock_sonos(), Instant::now());

        let restore = restored.spawn_saved_session_restore().unwrap();
        tokio::time::timeout(Duration::from_secs(1), restore)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            restored.input().active_name().as_deref(),
            Some("Mock Monitor")
        );
        assert_eq!(
            restored.active_output(),
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:mock-sonos".into(),
                device_name: "Mock Sonos".into(),
            })
        );
        assert_eq!(restored.output_volume(), 37);
        assert_eq!(restored.eq_gains_db(), [1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(restored.input_sample_rate_hz(), 48_000);
        assert_eq!(
            restored.mock_log.lock().await.as_slice(),
            ["Mock Sonos:start", "Mock Sonos:volume:37"]
        );

        restored.shutdown().await;
        drop(restored);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = std::fs::remove_file(path);
    }

    /// The restore must re-check `still_current()` after waiting for
    /// `config_lock`: a user activation that completed while it waited must
    /// not be overwritten by the saved output.
    #[tokio::test]
    async fn a_user_activation_during_restore_wins_over_the_saved_output() {
        let path = settings_test_path("restore-race");
        let initial = CoreState::new_persistent(path.clone());
        initial.remember_output(ActiveOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into(),
            device_name: "Mock Sonos".into(),
        });
        initial.persist_settings_now();
        drop(initial);

        let mut state = CoreState::new_persistent(path.clone());
        state.set_mock(true);
        state.config.restore_delay = Duration::from_millis(1);
        state
            .sonos
            .lock()
            .await
            .upsert(mock_sonos(), Instant::now());
        *state.airplay.lock() = vec![CatalogDevice {
            id: "ap-living".into(),
            name: "Living Room AirPlay".into(),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: String::new(),
        }];

        // Hold the configuration lock as a request handler would, let the
        // restore task pass its pre-lock check, then activate another output
        // before releasing the lock.
        let configuration = state.config_lock.lock().await;
        let restore = state.spawn_saved_session_restore().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        crate::session::activate(&state, "airplay", "ap-living")
            .await
            .unwrap();
        drop(configuration);

        tokio::time::timeout(Duration::from_secs(1), restore)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            state.active_output().map(|o| o.device_id).as_deref(),
            Some("ap-living"),
            "the user's choice must survive the saved-session restore"
        );
        assert_eq!(
            state.mock_log.lock().await.as_slice(),
            ["Living Room AirPlay:start", "Living Room AirPlay:volume:50"]
        );

        state.shutdown().await;
        drop(state);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = std::fs::remove_file(path);
    }

    /// Two `POST /api/outputs/active` requests racing through the real
    /// handler path: exactly one output is live at the end and the other
    /// was cleanly stopped, never left playing.
    #[tokio::test]
    async fn concurrent_output_activations_leave_exactly_one_live() {
        let state = CoreState::new_mock().await;
        let activate = |transport: &'static str, id: &'static str| {
            let state = state.clone();
            tokio::spawn(async move {
                let _configuration = state.config_lock.lock().await;
                crate::session::activate(&state, transport, id).await
            })
        };
        let a = activate("sonos", "uuid:mock-sonos");
        let b = activate("airplay", "ap-living");
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();

        let log = state.mock_log.lock().await.clone();
        let starts = log.iter().filter(|l| l.ends_with(":start")).count();
        let stops = log.iter().filter(|l| l.ends_with(":stop")).count();
        assert_eq!((starts, stops), (2, 1), "{log:?}");
        let live = state.active_output().unwrap();
        let stopped = log
            .iter()
            .find_map(|l| l.strip_suffix(":stop"))
            .unwrap()
            .to_string();
        assert_ne!(live.device_name, stopped, "{log:?}");
        assert_eq!(
            state.output().sender_name().await.as_deref(),
            Some(live.device_name.as_str())
        );
        state.shutdown().await;
    }

    // ---- first-run tuning ----------------------------------------------

    fn outcome(
        recommended_buffer_ms: u32,
        recommended_quality: crate::dsp::DspQuality,
    ) -> crate::audio_benchmark::BenchmarkOutcome {
        crate::audio_benchmark::BenchmarkOutcome {
            report: on_air_api::AudioBenchmark {
                input_hz: 44_100,
                output_hz: 48_000,
                duration_ms: 1,
                pipeline_p95_ms: 1.0,
                scheduler_p95_ms: 0.0,
                worst_case_ms: 1.0,
                recommended_buffer_ms,
                within_buffer_budget: true,
                recommended_quality,
            },
            profile_costs_ms: [0.1, 0.2, 0.3],
            recommended_quality,
        }
    }

    #[tokio::test]
    async fn first_run_tuning_applies_the_verdict_to_untouched_settings_and_records_it() {
        use crate::tuning::AppliedTuning;
        let state = CoreState::new_mock().await;
        state.input().activate("Mock Monitor").await.unwrap();
        let applied = state
            .record_first_run_tuning(&outcome(100, crate::dsp::DspQuality::Balanced))
            .await
            .unwrap();
        assert_eq!(
            applied,
            AppliedTuning {
                buffer: true,
                quality: true
            }
        );
        let saved = state.saved_settings();
        assert_eq!(saved.capture_buffer_ms, 100);
        assert_eq!(saved.dsp_quality, Some(crate::dsp::DspQuality::Balanced));
        assert_eq!(
            saved.recommended_dsp_quality,
            Some(crate::dsp::DspQuality::Balanced)
        );
        let record = saved.first_run_tuning.expect("tuning is recorded");
        assert!(record.applied_buffer && record.applied_quality);
        assert_eq!(record.recommended_buffer_ms, 100);
        assert!(!state.first_run_tuning_pending());
        assert_eq!(state.dsp_quality(), crate::dsp::DspQuality::Balanced);
    }

    #[tokio::test]
    async fn first_run_tuning_never_overwrites_a_users_choices() {
        use crate::tuning::AppliedTuning;
        let state = CoreState::new_mock().await;
        state.input().activate("Mock Monitor").await.unwrap();
        state.apply_capture_buffer(200).await.unwrap();
        state
            .apply_dsp_quality(crate::dsp::DspQuality::High)
            .await
            .unwrap();
        let applied = state
            .record_first_run_tuning(&outcome(100, crate::dsp::DspQuality::Balanced))
            .await
            .unwrap();
        assert_eq!(applied, AppliedTuning::default());
        let saved = state.saved_settings();
        assert_eq!(saved.capture_buffer_ms, 200);
        assert_eq!(saved.dsp_quality, Some(crate::dsp::DspQuality::High));
        assert_eq!(
            saved.recommended_dsp_quality,
            Some(crate::dsp::DspQuality::Balanced)
        );
        let record = saved.first_run_tuning.expect("tuning is recorded");
        assert!(!record.applied_buffer && !record.applied_quality);
    }

    #[tokio::test]
    async fn a_tuning_apply_that_cannot_take_effect_records_nothing_and_retries_later() {
        let mut state = CoreState::new_mock().await;
        state.input().activate("Mock Monitor").await.unwrap();
        // Hardware mode without that device: the capture restart fails and
        // the whole apply must roll back.
        state.set_mock(false);
        let error = state
            .record_first_run_tuning(&outcome(100, crate::dsp::DspQuality::Balanced))
            .await
            .unwrap_err();
        assert!(matches!(error, ApplyError::Failed(_)), "{error}");
        let saved = state.saved_settings();
        assert_eq!(saved.capture_buffer_ms, 0, "rolled back");
        assert_eq!(saved.dsp_quality, None);
        assert_eq!(saved.recommended_dsp_quality, None);
        assert_eq!(saved.first_run_tuning, None);
        assert!(state.first_run_tuning_pending(), "a later run retries");
    }
}
