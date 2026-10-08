//! `CoreState`: the device catalogs, the user's settings, pairing, and the
//! two sessions (input and exclusive output) that own live audio.

use crate::cd::CdDeck;
use crate::events::{emit_catalog_diff, WsEvent};
use crate::pairing::PairingState;
use crate::pipeline::input::InputWiring;
use crate::pipeline::{InputError, InputSession};
use crate::sender::airplay_mdns::CatalogDevice;
use crate::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, MockBluetoothAdapter, RecordingPcmSink,
    SystemBluetoothAdapter,
};
use crate::sender::sonos::discovery::{DeviceRegistry, SonosDevice};
use crate::sender::{AudioSender, SenderError};
use crate::session::OutputSession;
use crate::settings::{SavedSettings, SettingsPersistence};
use bytes::Bytes;
use parking_lot::Mutex as StdMutex;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};

pub use crate::session::ActiveOutput;

type BluetoothDeviceCache = Option<(Instant, Vec<BluetoothDevice>)>;

/// Tunables that used to be `cfg!(test)` branches.
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

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
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
    // Sessions. Each owns its live resources under its own transition lock.
    output: OutputSession,
    input: InputSession,
    /// Serializes configuration changes that span both sessions (an output
    /// switch, an input switch, a sample-rate change, the saved-session
    /// restore) so they apply in a single order.
    pub(crate) config_lock: Arc<Mutex<()>>,
    pub(crate) cd: CdDeck,

    // Buses.
    /// Pipeline PCM: processed mono L16 at the input rate.
    pub audio_tx: broadcast::Sender<Bytes>,
    pub ws_tx: broadcast::Sender<WsEvent>,

    // Catalogs.
    pub(crate) sonos: Arc<Mutex<DeviceRegistry>>,
    pub(crate) airplay: Arc<StdMutex<Vec<CatalogDevice>>>,
    pub(crate) owntone_base: Arc<StdMutex<String>>,
    pub(crate) bluetooth: Arc<dyn BluetoothAdapter>,
    bluetooth_cache: Arc<Mutex<BluetoothDeviceCache>>,

    // Settings.
    pub(crate) eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub(crate) target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub(crate) output_sample_rate_hz: Arc<StdMutex<u32>>,
    output_volume: Arc<AtomicU8>,
    /// Controls whether remote clients may start or configure audio work. The
    /// lightweight HTTP and discovery services remain online while disabled.
    pub service_enabled: Arc<AtomicBool>,
    saved_settings: Arc<StdMutex<SavedSettings>>,
    settings_persistence: Option<SettingsPersistence>,
    settings_revision: Arc<AtomicU64>,
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

    // Mode, config and host plumbing.
    pub(crate) mock: bool,
    pub config: CoreConfig,
    pub(crate) background: Arc<StdMutex<BackgroundTasks>>,
    /// Calls made on mock senders, for tests.
    pub mock_log: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub(crate) pcm_sink: Arc<RecordingPcmSink>,
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;
// Network speakers buffer and read live radio streams in bursts. At 1024 mono
// frames per chunk this retains roughly 12 seconds / 1 MiB at 44.1 kHz, enough
// to absorb Sonos read pauses without making the audio path unbounded.
const AUDIO_BROADCAST_CAPACITY: usize = 512;

fn bluetooth_ids(devices: &[BluetoothDevice]) -> Vec<(String, String)> {
    devices
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect()
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
        let mut state = Self::from_saved_settings(saved, Some(SettingsPersistence::new(path)));
        state.pairing = Arc::new(StdMutex::new(pairing));
        state
    }

    fn from_saved_settings(
        saved_settings: SavedSettings,
        settings_persistence: Option<SettingsPersistence>,
    ) -> Self {
        let (audio_tx, _) = broadcast::channel(AUDIO_BROADCAST_CAPACITY);
        let (ws_tx, _) = broadcast::channel(64);
        let cd = CdDeck::new();
        cd.set_event_sink(ws_tx.clone());
        let eq_gains_db = Arc::new(StdMutex::new(saved_settings.eq_gains_db));
        let target_sample_rate_hz = Arc::new(StdMutex::new(saved_settings.input_sample_rate_hz));
        let output_sample_rate_hz = Arc::new(StdMutex::new(saved_settings.output_sample_rate_hz));
        let output = OutputSession::new(ws_tx.clone(), output_sample_rate_hz.clone());
        let input = InputSession::new(InputWiring {
            audio_tx: audio_tx.clone(),
            ws_tx: ws_tx.clone(),
            eq_gains_db: eq_gains_db.clone(),
            input_rate_hz: target_sample_rate_hz.clone(),
            cd: cd.clone(),
        });
        CoreState {
            output,
            input,
            config_lock: Arc::new(Mutex::new(())),
            cd,
            audio_tx,
            ws_tx,
            sonos: Arc::new(Mutex::new(DeviceRegistry::new())),
            airplay: Arc::new(StdMutex::new(Vec::new())),
            owntone_base: Arc::new(StdMutex::new("http://127.0.0.1:3689".into())),
            bluetooth: Arc::new(SystemBluetoothAdapter::from_host()),
            bluetooth_cache: Arc::new(Mutex::new(None)),
            eq_gains_db,
            target_sample_rate_hz,
            output_sample_rate_hz,
            output_volume: Arc::new(AtomicU8::new(saved_settings.volume)),
            service_enabled: Arc::new(AtomicBool::new(saved_settings.service_enabled)),
            saved_settings: Arc::new(StdMutex::new(saved_settings)),
            settings_persistence,
            settings_revision: Arc::new(AtomicU64::new(0)),
            restoring_settings: Arc::new(AtomicBool::new(false)),
            require_auth: false,
            pairing: Arc::new(StdMutex::new(PairingState::new())),
            pairing_verify_lock: Arc::new(Mutex::new(())),
            stream_clients: Arc::new(AtomicUsize::new(0)),
            stream_progress: Arc::new(AtomicU64::new(0)),
            serve_port: Arc::new(AtomicU16::new(crate::DEFAULT_PORT)),
            mock: false,
            config: CoreConfig::default(),
            background: Arc::new(StdMutex::new(BackgroundTasks::default())),
            mock_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            pcm_sink: Arc::new(RecordingPcmSink::default()),
        }
    }

    /// E2E / CI: fake inputs and transports, fixed pairing PIN, no hardware.
    pub async fn new_mock() -> Self {
        let mut state = Self::new();
        state.set_mock(true);
        state.pairing = Arc::new(StdMutex::new(PairingState::mock()));
        state.input.set_mock_inputs(vec!["Mock Monitor".into()]);
        *state.airplay.lock() = vec![CatalogDevice {
            id: "ap-living".into(),
            name: "Living Room AirPlay".into(),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: String::new(),
        }];
        state.bluetooth = Arc::new(MockBluetoothAdapter::with_devices(vec![
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
        state.sonos.lock().await.upsert(sonos, Instant::now());
        state
    }

    /// Hardware-free mode: senders are fakes, inputs come from the mock
    /// list, and the laptop speakers are never touched.
    pub fn set_mock(&mut self, mock: bool) {
        self.mock = mock;
        self.output.set_manage_local_sink(!mock);
        self.input.set_mock(mock);
    }

    pub fn is_mock(&self) -> bool {
        self.mock
    }

    // ---- sessions -------------------------------------------------------

    /// The exclusive-output session.
    pub fn output(&self) -> &OutputSession {
        &self.output
    }

    /// The live-input session.
    pub fn input(&self) -> &InputSession {
        &self.input
    }

    /// Identity of the output that owns exclusivity (starting, live or
    /// failed).
    pub fn active_output(&self) -> Option<ActiveOutput> {
        self.output.active()
    }

    /// Activate an input and remember it. The caller holds `config_lock`.
    pub(crate) async fn activate_input_locked(&self, name: &str) -> Result<(), InputError> {
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

    pub(crate) fn remember_eq(&self, gains: [f32; 5]) {
        self.update_saved_settings(|saved| saved.eq_gains_db = gains);
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

    /// Audit finding #1: the restore used to check `still_current()` before
    /// waiting for `config_lock`, so a user activation that completed while
    /// it waited was overwritten by the saved output.
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
}
