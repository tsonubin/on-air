use crate::api::ws::WsEvent;
use crate::cd::CdDeck;
use crate::pairing::PairingState;
use crate::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, MockBluetoothAdapter, RecordingPcmSink,
    SystemBluetoothAdapter,
};
use crate::sender::sonos::discovery::{DeviceRegistry, SonosDevice};
use crate::sender::{AudioSender, OutputFormat, SenderError};
use crate::settings::{SavedSettings, SettingsPersistence};
use bytes::Bytes;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};

type BluetoothDeviceCache = Option<(Instant, Vec<BluetoothDevice>)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDevice {
    pub id: String,
    pub name: String,
    pub needs_pair: bool,
    pub paired: bool,
    pub kind: &'static str,
    pub member_count: u8,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ActiveOutput {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone)]
pub struct CoreState {
    /// Serializes configuration changes that rebuild capture or sender state.
    pub config_lock: Arc<Mutex<()>>,
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub input_supported_hz: Arc<StdMutex<Vec<u32>>>,
    pub output_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
    pub outputs: Arc<Mutex<DeviceRegistry>>,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub mock: bool,
    pub mock_inputs: Arc<StdMutex<Vec<String>>>,
    pub active_input: Arc<StdMutex<Option<String>>>,
    pub cd: CdDeck,
    pub airplay_outputs: Arc<StdMutex<Vec<CatalogDevice>>>,
    pub bluetooth: Arc<dyn BluetoothAdapter>,
    bluetooth_cache: Arc<Mutex<BluetoothDeviceCache>>,
    pub pcm_sink: Arc<RecordingPcmSink>,
    pub require_auth: bool,
    pub pairing: Arc<StdMutex<PairingState>>,
    pub active_output: Arc<StdMutex<Option<ActiveOutput>>>,
    /// Last negotiated format, published without holding the live sender lock.
    pub(crate) active_output_format: Arc<StdMutex<Option<(ActiveOutput, OutputFormat)>>>,
    pub output_volume: Arc<AtomicU8>,
    /// Controls whether remote clients may start or configure audio work. The
    /// lightweight HTTP and discovery services remain online while disabled.
    pub service_enabled: Arc<AtomicBool>,
    pub stream_generation: Arc<AtomicU64>,
    pub stream_clients: Arc<AtomicUsize>,
    /// Monotonic heartbeat advanced only when a live stream body is polled.
    /// Sonos recovery uses it to distinguish a connected reader from one that
    /// has stopped draining audio.
    pub stream_progress: Arc<AtomicU64>,
    pub mock_log: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub owntone_base: Arc<StdMutex<String>>,
    saved_settings: Arc<StdMutex<SavedSettings>>,
    settings_persistence: Option<SettingsPersistence>,
    settings_revision: Arc<AtomicU64>,
    restoring_settings: Arc<AtomicBool>,
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;
// Network speakers buffer and read live radio streams in bursts. At 1024 mono
// frames per chunk this retains roughly 12 seconds / 1 MiB at 44.1 kHz, enough
// to absorb Sonos read pauses without making the audio path unbounded.
const AUDIO_BROADCAST_CAPACITY: usize = 512;
const AIRPLAY_EMPTY_SCANS_BEFORE_EVICTION: u8 = 3;
const SONOS_NAME_LOOKUP_CONCURRENCY: usize = 4;
const MAX_SONOS_NAME_LOOKUPS_PER_SCAN: usize = 64;

fn apply_airplay_scan(
    current: &mut Vec<CatalogDevice>,
    found: Vec<CatalogDevice>,
    consecutive_empty_scans: &mut u8,
) {
    if found.is_empty() {
        *consecutive_empty_scans = consecutive_empty_scans.saturating_add(1);
        if *consecutive_empty_scans >= AIRPLAY_EMPTY_SCANS_BEFORE_EVICTION {
            current.clear();
        }
        return;
    }
    *consecutive_empty_scans = 0;
    if *current != found {
        *current = found;
    }
}

fn spawn_sonos_name_lookup(
    tasks: &mut tokio::task::JoinSet<SonosDevice>,
    http: &reqwest::Client,
    mut device: SonosDevice,
) {
    let http = http.clone();
    tasks.spawn(async move {
        if let Some(name) =
            crate::sender::sonos::discovery::fetch_friendly_name(&http, &device.location).await
        {
            device.friendly_name = name;
        }
        device
    });
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
        CoreState {
            config_lock: Arc::new(Mutex::new(())),
            active_sender: Arc::new(Mutex::new(None)),
            eq_gains_db: Arc::new(StdMutex::new(saved_settings.eq_gains_db)),
            target_sample_rate_hz: Arc::new(StdMutex::new(saved_settings.input_sample_rate_hz)),
            input_supported_hz: Arc::new(StdMutex::new(crate::dsp::rates::INPUT_RATES_HZ.to_vec())),
            output_sample_rate_hz: Arc::new(StdMutex::new(saved_settings.output_sample_rate_hz)),
            audio_tx,
            capture: Arc::new(Mutex::new(None)),
            outputs: Arc::new(Mutex::new(DeviceRegistry::new())),
            ws_tx,
            mock: false,
            mock_inputs: Arc::new(StdMutex::new(Vec::new())),
            active_input: Arc::new(StdMutex::new(None)),
            cd,
            airplay_outputs: Arc::new(StdMutex::new(Vec::new())),
            bluetooth: Arc::new(SystemBluetoothAdapter::from_host()),
            bluetooth_cache: Arc::new(Mutex::new(None)),
            pcm_sink: Arc::new(RecordingPcmSink::default()),
            require_auth: false,
            pairing: Arc::new(StdMutex::new(PairingState::new())),
            active_output: Arc::new(StdMutex::new(None)),
            active_output_format: Arc::new(StdMutex::new(None)),
            output_volume: Arc::new(AtomicU8::new(saved_settings.volume)),
            service_enabled: Arc::new(AtomicBool::new(saved_settings.service_enabled)),
            stream_generation: Arc::new(AtomicU64::new(0)),
            stream_clients: Arc::new(AtomicUsize::new(0)),
            stream_progress: Arc::new(AtomicU64::new(0)),
            mock_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            owntone_base: Arc::new(StdMutex::new("http://127.0.0.1:3689".into())),
            saved_settings: Arc::new(StdMutex::new(saved_settings)),
            settings_persistence,
            settings_revision: Arc::new(AtomicU64::new(0)),
            restoring_settings: Arc::new(AtomicBool::new(false)),
        }
    }

    fn update_saved_settings(&self, update: impl FnOnce(&mut SavedSettings)) {
        let mut saved = self.saved_settings.lock().unwrap();
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

    pub(crate) fn remember_volume(&self, volume: u8) {
        self.output_volume.store(volume, Ordering::Release);
        self.update_saved_settings(|saved| saved.volume = volume);
    }

    pub(crate) fn remember_eq(&self, gains: [f32; 5]) {
        self.update_saved_settings(|saved| saved.eq_gains_db = gains);
    }

    pub(crate) fn remember_sample_rates(&self) {
        let input = *self.target_sample_rate_hz.lock().unwrap();
        let output = *self.output_sample_rate_hz.lock().unwrap();
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
        let saved = self.saved_settings.lock().unwrap();
        if let Err(error) = persistence.save_now(&saved) {
            eprintln!("could not flush on-air settings: {error}");
        }
    }

    pub fn spawn_saved_session_restore(&self) -> Option<tokio::task::JoinHandle<()>> {
        let saved = self.saved_settings.lock().unwrap().clone();
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
        let revision = self.settings_revision.load(Ordering::Acquire);
        Some(tokio::spawn(async move {
            state.restore_saved_session(saved, revision).await;
            state.restoring_settings.store(false, Ordering::Release);
        }))
    }

    async fn restore_saved_session(&self, saved: SavedSettings, revision: u64) {
        let initial_delay = if cfg!(test) {
            Duration::from_millis(1)
        } else {
            Duration::from_millis(500)
        };
        tokio::time::sleep(initial_delay).await;

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
                    crate::api::inputs::activate_input_named(self, input).await
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
            if let Some((_, error)) = last_error {
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
                crate::session::activate(self, &output.transport, &output.device_id).await
            };
            match result {
                Ok(()) => return,
                Err(crate::session::ActivateError::BadRequest(error)) => {
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
                "could not restore saved {} output {:?}: {error:?}",
                output.transport, output.device_name
            );
        }
    }

    /// E2E / CI: fake inputs and transports, fixed pairing PIN, no hardware.
    pub async fn new_mock() -> Self {
        let mut state = Self::new();
        state.mock = true;
        state.pairing = Arc::new(StdMutex::new(PairingState::mock()));
        *state.mock_inputs.lock().unwrap() = vec!["Mock Monitor".into()];
        *state.airplay_outputs.lock().unwrap() = vec![CatalogDevice {
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
        state.outputs.lock().await.upsert(sonos, Instant::now());
        state
    }

    pub fn spawn_airplay_discovery(&self) -> tokio::task::JoinHandle<()> {
        let outputs = self.airplay_outputs.clone();
        let base = self.owntone_base.clone();
        tokio::spawn(async move {
            let owntone_http = crate::sender::sonos::soap::lan_client(Duration::from_millis(500));
            let mut consecutive_empty_scans = 0;
            loop {
                let mut found =
                    crate::sender::airplay_mdns::search_mdns(Duration::from_secs(2)).await;
                let url = base.lock().unwrap().clone();
                if let Ok(owntone) =
                    crate::sender::airplay::fetch_owntone_outputs_with_client(&owntone_http, &url)
                        .await
                {
                    crate::sender::airplay_mdns::merge_owntone(&mut found, owntone);
                }
                apply_airplay_scan(
                    &mut outputs.lock().unwrap(),
                    found,
                    &mut consecutive_empty_scans,
                );
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        })
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
        *cache = Some((Instant::now(), devices.clone()));
        Ok(devices)
    }

    pub async fn invalidate_bluetooth_cache(&self) {
        *self.bluetooth_cache.lock().await = None;
    }

    pub async fn activate_sender(
        &self,
        new_sender: Box<dyn AudioSender>,
    ) -> Result<(), SenderError> {
        self.activate_sender_as(new_sender, None).await
    }

    fn invalidate_radio_streams(&self) {
        self.stream_generation.fetch_add(1, Ordering::AcqRel);
    }

    async fn apply_local_sink_for(&self, identity: &ActiveOutput) {
        if self.mock {
            return;
        }
        let transport = identity.transport.clone();
        let device_id = identity.device_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            crate::pipeline::local_sink::apply_for_transport(&transport, &device_id);
        })
        .await;
    }

    async fn restore_local_sink(&self) {
        if self.mock {
            return;
        }
        let _ = tokio::task::spawn_blocking(|| {
            crate::pipeline::local_sink::restore_local_speakers();
        })
        .await;
    }

    fn publish_output_format(&self, sender: &dyn AudioSender, identity: &ActiveOutput) {
        let format = sender.output_format();
        if let Some(format) = &format {
            *self.output_sample_rate_hz.lock().unwrap() = format.sample_rate_hz;
        }
        *self.active_output_format.lock().unwrap() =
            format.map(|format| (identity.clone(), format));
    }

    pub async fn activate_sender_as(
        &self,
        new_sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
    ) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        let mut previous_sender = guard.take();
        let previous_identity = self.active_output.lock().unwrap().take();
        if let Some(current) = previous_sender.as_mut() {
            let transport = previous_identity
                .as_ref()
                .map(|o| o.transport.clone())
                .unwrap_or_else(|| current.transport().to_string());
            let device_name = previous_identity
                .as_ref()
                .map(|o| o.device_name.clone())
                .unwrap_or_else(|| current.name().to_string());
            if let Err(error) = current.stop().await {
                *self.active_output.lock().unwrap() = previous_identity;
                *guard = previous_sender;
                return Err(SenderError(format!(
                    "could not stop the active output before switching: {error}"
                )));
            }
            let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
                transport,
                device_name,
                active: false,
            });
            self.invalidate_radio_streams();
        }
        *self.active_output_format.lock().unwrap() = None;
        let mut new_sender = new_sender;
        let identity = identity.unwrap_or_else(|| ActiveOutput {
            transport: new_sender.transport().to_string(),
            device_id: String::new(),
            device_name: new_sender.name().to_string(),
        });
        // GET /stream/audio.wav is 404 until this is set. Sonos Play pulls
        // the URI immediately, so the radio must be live before SOAP starts.
        *self.active_output.lock().unwrap() = Some(identity.clone());
        if let Err(e) = new_sender.start().await {
            *self.active_output.lock().unwrap() = None;
            let cleanup_error = new_sender.stop().await.err();
            self.invalidate_radio_streams();
            if let Some(cleanup) = cleanup_error {
                // A receiver may still be playing after a partial start. Keep
                // ownership for another stop attempt; never resume a second one.
                *self.active_output.lock().unwrap() = Some(identity);
                *guard = Some(new_sender);
                return Err(SenderError(format!(
                    "{e}; cleanup after failed start also failed: {cleanup}"
                )));
            }
            if let Some(mut previous) = previous_sender {
                *self.active_output.lock().unwrap() = previous_identity.clone();
                if let Err(restart) = previous.start().await {
                    if let Err(cleanup) = previous.stop().await {
                        *guard = Some(previous);
                        return Err(SenderError(format!(
                            "{e}; restoring previous output failed: {restart}; cleanup failed: {cleanup}"
                        )));
                    }
                    *self.active_output.lock().unwrap() = None;
                    self.restore_local_sink().await;
                    return Err(SenderError(format!(
                        "{e}; restoring previous output failed: {restart}"
                    )));
                } else {
                    if let Some(previous_identity) = previous_identity.as_ref() {
                        self.publish_output_format(previous.as_ref(), previous_identity);
                        self.apply_local_sink_for(previous_identity).await;
                        let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
                            transport: previous_identity.transport.clone(),
                            device_name: previous_identity.device_name.clone(),
                            active: true,
                        });
                    }
                    *guard = Some(previous);
                }
            } else {
                self.restore_local_sink().await;
            }
            return Err(e);
        }
        self.publish_output_format(new_sender.as_ref(), &identity);
        self.apply_local_sink_for(&identity).await;
        let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
            transport: identity.transport.clone(),
            device_name: identity.device_name.clone(),
            active: true,
        });
        *guard = Some(new_sender);
        Ok(())
    }

    pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            let previous = self.active_output.lock().unwrap().take();
            let transport = previous
                .as_ref()
                .map(|o| o.transport.clone())
                .unwrap_or_else(|| current.transport().to_string());
            let device_name = previous
                .as_ref()
                .map(|o| o.device_name.clone())
                .unwrap_or_else(|| current.name().to_string());
            if let Err(error) = current.stop().await {
                *self.active_output.lock().unwrap() = previous;
                *guard = Some(current);
                return Err(error);
            }
            self.invalidate_radio_streams();
            *self.active_output_format.lock().unwrap() = None;
            self.restore_local_sink().await;
            let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
                transport,
                device_name,
                active: false,
            });
        }
        Ok(())
    }

    /// Stop all live audio resources before the host process exits.
    pub async fn shutdown(&self) {
        self.persist_settings_now();
        let _configuration = self.config_lock.lock().await;
        if let Err(error) = self.deactivate_sender().await {
            eprintln!("could not stop active output cleanly; forcing local shutdown: {error}");
            let abandoned = self.active_sender.lock().await.take();
            drop(abandoned);
            self.active_output.lock().unwrap().take();
            self.invalidate_radio_streams();
        }
        if let Some(capture) = self.capture.lock().await.take() {
            let _ = tokio::task::spawn_blocking(move || capture.stop()).await;
        }
        *self.active_input.lock().unwrap() = None;
        *self.input_supported_hz.lock().unwrap() = crate::dsp::rates::INPUT_RATES_HZ.to_vec();
        self.restore_local_sink().await;
    }

    /// Spawns a background task that periodically SSDP-searches for Sonos
    /// devices and merges results into `self.outputs`, expiring stale entries.
    pub fn spawn_sonos_discovery(&self) -> tokio::task::JoinHandle<()> {
        use crate::sender::sonos::discovery::{
            search_mdns, search_once, DEVICE_TTL, DISCOVERY_INTERVAL,
        };

        let outputs = self.outputs.clone();
        let ws_tx = self.ws_tx.clone();
        tokio::spawn(async move {
            let http = crate::sender::sonos::soap::lan_client(Duration::from_secs(2));
            loop {
                let before: std::collections::HashSet<String> = {
                    let registry = outputs.lock().await;
                    registry.list().into_iter().map(|d| d.usn).collect()
                };

                let (ssdp, mdns) = tokio::join!(
                    search_once(Duration::from_secs(2)),
                    search_mdns(Duration::from_secs(2)),
                );
                let mut found = ssdp.unwrap_or_default();
                found.extend(mdns);
                let mut seen = std::collections::HashSet::new();
                let unique = found
                    .into_iter()
                    .filter(|device| {
                        seen.insert(crate::sender::sonos::discovery::rincon_key(&device.usn))
                    })
                    .take(MAX_SONOS_NAME_LOOKUPS_PER_SCAN);
                let mut pending = unique;
                let mut name_tasks = tokio::task::JoinSet::new();
                for device in pending.by_ref().take(SONOS_NAME_LOOKUP_CONCURRENCY) {
                    spawn_sonos_name_lookup(&mut name_tasks, &http, device);
                }
                let mut named = Vec::new();
                while let Some(result) = name_tasks.join_next().await {
                    if let Ok(device) = result {
                        named.push(device);
                    }
                    if let Some(device) = pending.next() {
                        spawn_sonos_name_lookup(&mut name_tasks, &http, device);
                    }
                }
                named.sort_by(|a, b| a.usn.cmp(&b.usn));

                let topology = named
                    .iter()
                    .map(crate::sender::sonos::discovery::soap_ip)
                    .find(|ip| ip.is_ipv4())
                    .or_else(|| named.first().map(|d| d.ip));
                let topology = match topology {
                    Some(ip) => crate::sender::sonos::discovery::fetch_zone_groups(&http, ip).await,
                    None => None,
                };

                let now = Instant::now();
                {
                    let mut registry = outputs.lock().await;
                    for device in named {
                        if !before.contains(&device.usn) {
                            let _ = ws_tx.send(WsEvent::DeviceJoined {
                                transport: "sonos".to_string(),
                                id: device.usn.clone(),
                                name: device.friendly_name.clone(),
                            });
                        }
                        registry.upsert(device, now);
                    }
                    if let Some(groups) = topology {
                        registry.apply_zone_groups(&groups);
                    }
                    registry.expire_stale(now, DEVICE_TTL);

                    let after: std::collections::HashSet<String> =
                        registry.list().into_iter().map(|d| d.usn).collect();
                    for left in before.difference(&after) {
                        let _ = ws_tx.send(WsEvent::DeviceLeft {
                            transport: "sonos".to_string(),
                            id: left.clone(),
                        });
                    }
                }
                tokio::time::sleep(DISCOVERY_INTERVAL).await;
            }
        })
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

    fn receiver(id: &str) -> CatalogDevice {
        CatalogDevice {
            id: id.into(),
            name: format!("Receiver {id}"),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: "192.168.1.2".into(),
        }
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

    #[test]
    fn airplay_catalog_tolerates_transient_misses_then_evicts_stale_receivers() {
        let mut current = vec![receiver("old")];
        let mut misses = 0;

        apply_airplay_scan(&mut current, Vec::new(), &mut misses);
        apply_airplay_scan(&mut current, Vec::new(), &mut misses);
        assert_eq!(current, vec![receiver("old")]);

        apply_airplay_scan(&mut current, vec![receiver("new")], &mut misses);
        assert_eq!(current, vec![receiver("new")]);
        assert_eq!(misses, 0);

        for _ in 0..AIRPLAY_EMPTY_SCANS_BEFORE_EVICTION {
            apply_airplay_scan(&mut current, Vec::new(), &mut misses);
        }
        assert!(current.is_empty());
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
        *initial.eq_gains_db.lock().unwrap() = [1.0, 2.0, 3.0, 4.0, 5.0];
        initial.remember_eq([1.0, 2.0, 3.0, 4.0, 5.0]);
        *initial.target_sample_rate_hz.lock().unwrap() = 48_000;
        *initial.output_sample_rate_hz.lock().unwrap() = 44_100;
        initial.remember_sample_rates();
        initial.persist_settings_now();
        tokio::time::sleep(Duration::from_millis(250)).await;
        drop(initial);

        let mut restored = CoreState::new_persistent(path.clone());
        restored.mock = true;
        *restored.mock_inputs.lock().unwrap() = vec!["Mock Monitor".into()];
        let sonos = SonosDevice::discovered(
            "uuid:mock-sonos",
            "http://127.0.0.1:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "Mock Sonos",
        );
        restored.outputs.lock().await.upsert(sonos, Instant::now());

        let restore = restored.spawn_saved_session_restore().unwrap();
        tokio::time::timeout(Duration::from_secs(1), restore)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            restored.active_input.lock().unwrap().as_deref(),
            Some("Mock Monitor")
        );
        assert_eq!(
            restored.active_output.lock().unwrap().as_ref(),
            Some(&ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:mock-sonos".into(),
                device_name: "Mock Sonos".into(),
            })
        );
        assert_eq!(restored.output_volume.load(Ordering::Acquire), 37);
        assert_eq!(
            *restored.eq_gains_db.lock().unwrap(),
            [1.0, 2.0, 3.0, 4.0, 5.0]
        );
        assert_eq!(*restored.target_sample_rate_hz.lock().unwrap(), 48_000);
        assert_eq!(
            restored.mock_log.lock().await.as_slice(),
            ["Mock Sonos:start", "Mock Sonos:volume:37"]
        );

        restored.shutdown().await;
        drop(restored);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = std::fs::remove_file(path);
    }
}
