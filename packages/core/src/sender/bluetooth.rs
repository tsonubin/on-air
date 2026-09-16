#[path = "bluetooth_host.rs"]
mod host;
#[path = "bluetooth_pcm.rs"]
mod pcm;

pub use pcm::{
    BluetoothEndpoint, OpenedPcmSink, PcmOutput, PcmSink, RecordingPcmSink, SystemPcmOutput,
};

use crate::sender::{AudioSender, OutputFormat, SenderError};
use bytes::Bytes;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    /// Pulse/cpal output name used to open the PCM sink, once the OS has one.
    pub audio_endpoint: Option<String>,
}

pub trait BluetoothAdapter: Send + Sync {
    fn list(&self) -> Vec<BluetoothDevice>;
    fn pair(&self, id: &str) -> Result<(), SenderError>;
    fn connect(&self, id: &str) -> Result<BluetoothEndpoint, SenderError>;
    fn disconnect(&self, id: &str) -> Result<(), SenderError>;
    fn open_settings(&self) -> Result<(), SenderError>;
}

#[derive(Default)]
pub struct MockBluetoothAdapter {
    devices: Mutex<Vec<BluetoothDevice>>,
}

impl MockBluetoothAdapter {
    pub fn with_devices(devices: Vec<BluetoothDevice>) -> Self {
        MockBluetoothAdapter {
            devices: Mutex::new(devices),
        }
    }
}

impl BluetoothAdapter for MockBluetoothAdapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        self.devices.lock().unwrap().clone()
    }

    fn pair(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        let device = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| SenderError("bluetooth device not found".into()))?;
        device.paired = true;
        Ok(())
    }

    fn connect(&self, id: &str) -> Result<BluetoothEndpoint, SenderError> {
        let mut devices = self.devices.lock().unwrap();
        let device = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| SenderError("bluetooth device not found".into()))?;
        if !device.paired {
            return Err(SenderError("bluetooth device not paired".into()));
        }
        let endpoint = device
            .audio_endpoint
            .get_or_insert_with(|| device.id.clone())
            .clone();
        for d in devices.iter_mut() {
            d.connected = d.id == id;
        }
        Ok(BluetoothEndpoint::Native(endpoint))
    }

    fn disconnect(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(device) = devices.iter_mut().find(|d| d.id == id) {
            device.connected = false;
        }
        Ok(())
    }

    fn open_settings(&self) -> Result<(), SenderError> {
        Ok(())
    }
}

/// Production discovery and connection adapter. Native platforms identify
/// Bluetooth transports; a friendly name never selects the playback backend.
pub struct SystemBluetoothAdapter;

impl SystemBluetoothAdapter {
    pub fn from_host() -> Self {
        SystemBluetoothAdapter
    }
}

pub fn looks_like_a2dp_sink(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("bluez") || n.contains("bluetooth") || n.contains("a2dp")
}

/// `bluez_output.58_EA_1F_87_56_45.a2dp_sink` → `58:EA:1F:87:56:45`
pub fn pulse_sink_address(name: &str) -> Option<String> {
    let rest = name.strip_prefix("bluez_output.")?;
    let mac = rest.split('.').next()?;
    if mac.chars().filter(|c| *c == '_').count() != 5 {
        return None;
    }
    Some(mac.replace('_', ":").to_ascii_uppercase())
}

pub use host::{
    is_coreaudio_bluetooth_transport, looks_like_windows_bluetooth_id, parse_bluetoothctl_devices,
    parse_bluetoothctl_info,
};

/// Parse `pactl list sinks` (full form) into A2DP devices.
pub fn parse_pactl_list_sinks(text: &str) -> Vec<BluetoothDevice> {
    let mut devices = Vec::new();
    let mut name: Option<String> = None;
    let mut description = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink #") {
            if let Some(id) = name.take() {
                if looks_like_a2dp_sink(&id) {
                    let display = if description.is_empty() {
                        id.clone()
                    } else {
                        description.clone()
                    };
                    devices.push(pulse_device(id, display));
                }
            }
            description.clear();
        } else if let Some(value) = trimmed.strip_prefix("Name: ") {
            name = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("Description: ") {
            description = value.to_string();
        }
    }
    if let Some(id) = name {
        if looks_like_a2dp_sink(&id) {
            let display = if description.is_empty() {
                id.clone()
            } else {
                description
            };
            devices.push(pulse_device(id, display));
        }
    }
    devices
}

fn pulse_device(sink_name: String, display: String) -> BluetoothDevice {
    BluetoothDevice {
        id: pulse_sink_address(&sink_name).unwrap_or_else(|| sink_name.clone()),
        name: display,
        paired: true,
        connected: true,
        audio_endpoint: Some(sink_name),
    }
}

#[cfg(target_os = "linux")]
pub(super) fn pulse_a2dp_sinks() -> Vec<BluetoothDevice> {
    let output = match std::process::Command::new("timeout")
        .args(["2", "pactl", "list", "sinks"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_pactl_list_sinks(&String::from_utf8_lossy(&output.stdout))
}

impl BluetoothAdapter for SystemBluetoothAdapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        host::list()
    }

    fn pair(&self, id: &str) -> Result<(), SenderError> {
        host::pair(id)
    }

    fn connect(&self, id: &str) -> Result<BluetoothEndpoint, SenderError> {
        host::connect(id)
    }

    fn disconnect(&self, _id: &str) -> Result<(), SenderError> {
        Ok(())
    }

    fn open_settings(&self) -> Result<(), SenderError> {
        host::open_settings()
    }
}

/// Requested rates and initial app volume, retained across stop/start rollback.
#[derive(Debug, Clone, Copy)]
pub struct BluetoothPlaybackConfig {
    pub pipeline_hz: u32,
    pub output_hz: u32,
    pub volume: u8,
}

struct LivePlayback {
    opened: OpenedPcmSink,
    stop: Arc<AtomicBool>,
    pump: Option<tokio::task::JoinHandle<()>>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LivePlayback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        // Closing, not the last Arc drop, interrupts a writer blocked in PCM I/O.
        if let Err(error) = self.opened.sink.close() {
            eprintln!("could not close Bluetooth playback: {error}");
        }
    }
}

/// Owns connection readiness and one live playback lifetime. Construction does
/// no I/O, so Exclusive output can stop the previous sender before calling start.
pub struct BluetoothSender {
    device: BluetoothDevice,
    adapter: Arc<dyn BluetoothAdapter>,
    audio_tx: broadcast::Sender<Bytes>,
    output: Arc<dyn PcmOutput>,
    config: BluetoothPlaybackConfig,
    connection_attempted: bool,
    live: Option<LivePlayback>,
}

impl BluetoothSender {
    pub fn new(
        device: BluetoothDevice,
        adapter: Arc<dyn BluetoothAdapter>,
        audio_tx: broadcast::Sender<Bytes>,
        output: Arc<dyn PcmOutput>,
        config: BluetoothPlaybackConfig,
    ) -> Self {
        Self {
            device,
            adapter,
            audio_tx,
            output,
            config,
            connection_attempted: false,
            live: None,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for BluetoothSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        if self.live.is_some() || self.connection_attempted {
            return Err(SenderError(
                "bluetooth sender must be stopped before starting".into(),
            ));
        }
        self.connection_attempted = true;
        let adapter = self.adapter.clone();
        let id = self.device.id.clone();
        let output = self.output.clone();
        let config = self.config;
        let opened = tokio::task::spawn_blocking(move || {
            let endpoint = adapter.connect(&id)?;
            output.open(&endpoint, config.pipeline_hz, config.output_hz)
        })
        .await
        .map_err(|e| SenderError(format!("Bluetooth preparation task failed: {e}")))??;
        self.live = Some(LivePlayback {
            opened,
            stop: Arc::new(AtomicBool::new(false)),
            pump: None,
            writer: None,
        });
        // Apply retained volume before subscribing; rollback cannot emit a burst
        // at the default volume while the caller restores settings later.
        self.set_volume(config.volume).await?;
        let live = self.live.as_mut().unwrap();
        let mut rx = self.audio_tx.subscribe();
        let (writer_tx, writer_rx) = sync_channel::<Bytes>(8);
        let sink = live.opened.sink.clone();
        let writer_stop = live.stop.clone();
        live.writer = Some(std::thread::spawn(move || {
            while !writer_stop.load(Ordering::Acquire) {
                match writer_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(chunk) => {
                        if writer_stop.load(Ordering::Acquire) {
                            break;
                        }
                        if let Err(error) = sink.write(&chunk) {
                            if !writer_stop.load(Ordering::Acquire) {
                                eprintln!("bluetooth audio sink stopped: {error}");
                            }
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
        let pump_stop = live.stop.clone();
        live.pump = Some(tokio::spawn(async move {
            while !pump_stop.load(Ordering::Acquire) {
                match rx.recv().await {
                    Ok(chunk) => match writer_tx.try_send(chunk) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        if let Some(live) = self.live.as_mut() {
            live.stop.store(true, Ordering::Release);
            if let Some(pump) = live.pump.take() {
                pump.abort();
            }
            let sink = live.opened.sink.clone();
            // Cancel I/O before joining; closing must not acquire a write lock.
            tokio::task::spawn_blocking(move || sink.close())
                .await
                .map_err(|e| SenderError(format!("Bluetooth close task failed: {e}")))??;
            if let Some(writer) = live.writer.take() {
                tokio::task::spawn_blocking(move || writer.join())
                    .await
                    .map_err(|e| SenderError(format!("Bluetooth writer task failed: {e}")))?
                    .map_err(|_| SenderError("Bluetooth writer thread panicked".into()))?;
            }
        }
        self.live = None;
        if self.connection_attempted {
            let adapter = self.adapter.clone();
            let id = self.device.id.clone();
            // Audio is already stopped. A connection cleanup error must not
            // make Exclusive output preserve a dead stream as still playing.
            match tokio::task::spawn_blocking(move || adapter.disconnect(&id)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("Bluetooth connection cleanup failed: {error}"),
                Err(error) => eprintln!("Bluetooth disconnect task failed: {error}"),
            }
            self.connection_attempted = false;
        }
        Ok(())
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        let volume = volume.min(100);
        if let Some(live) = self.live.as_ref() {
            let sink = live.opened.sink.clone();
            tokio::task::spawn_blocking(move || sink.set_volume(volume))
                .await
                .map_err(|e| SenderError(format!("Bluetooth volume task failed: {e}")))??;
        }
        self.config.volume = volume;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.device.name
    }
    fn transport(&self) -> &'static str {
        "bluetooth"
    }
    fn output_format(&self) -> Option<OutputFormat> {
        self.live.as_ref().map(|live| live.opened.format.clone())
    }
}
