use crate::dsp::bridge::RateBridge;
use crate::dsp::rates::{self, BLUETOOTH_RATES_HZ};
use crate::sender::{AudioSender, SenderError};
use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

pub trait BluetoothAdapter: Send + Sync {
    fn list(&self) -> Vec<BluetoothDevice>;
    fn pair(&self, id: &str) -> Result<(), SenderError>;
    fn connect(&self, id: &str) -> Result<(), SenderError>;
    fn disconnect(&self, id: &str) -> Result<(), SenderError>;
    fn set_volume(&self, id: &str, volume: u8) -> Result<(), SenderError>;
}

/// Destination for processed pipeline PCM (L16 LE bytes from `audio_tx`).
pub trait PcmSink: Send + Sync {
    fn write(&self, pcm: &[u8]);
}

#[derive(Default)]
pub struct RecordingPcmSink {
    pub chunks: Mutex<Vec<Vec<u8>>>,
}

impl RecordingPcmSink {
    pub fn byte_count(&self) -> usize {
        self.chunks.lock().unwrap().iter().map(|c| c.len()).sum()
    }
}

impl PcmSink for RecordingPcmSink {
    fn write(&self, pcm: &[u8]) {
        if !pcm.is_empty() {
            self.chunks.lock().unwrap().push(pcm.to_vec());
        }
    }
}

/// cpal output device used as the OS sink once a Bluetooth speaker is the system output.
pub struct CpalPcmSink {
    recorder: RecordingPcmSink,
    queue: Arc<Mutex<Vec<f32>>>,
    bridge: Mutex<RateBridge>,
    max_queue: usize,
    _stream: Option<cpal::Stream>,
}

impl CpalPcmSink {
    pub fn for_output_named(name: &str, pipeline_rate_hz: u32, output_rate_hz: u32) -> Self {
        let recorder = RecordingPcmSink::default();
        let queue = Arc::new(Mutex::new(Vec::<f32>::new()));
        let cb_queue = queue.clone();
        let host = cpal::default_host();
        let opened = host.output_devices().ok().and_then(|devices| {
            let device = devices.into_iter().find(|d| d.to_string() == name)?;
            let supported = device.default_output_config().ok()?;
            let mut config = supported.config();
            let ranges = supported_output_rate_ranges(&device);
            let can_output_rate = ranges
                .iter()
                .any(|(min, max)| output_rate_hz >= *min && output_rate_hz <= *max);
            if can_output_rate {
                config.sample_rate = output_rate_hz;
            }
            let device_rate = config.sample_rate;
            let channels = config.channels as usize;
            let err_fn = |err: cpal::Error| eprintln!("bluetooth cpal sink: {err}");
            let stream = device
                .build_output_stream(
                    config,
                    move |data: &mut [f32], _| {
                        let mut q = cb_queue.lock().unwrap();
                        let n = data.len().min(q.len());
                        for (slot, sample) in data.iter_mut().zip(q.drain(..n)) {
                            *slot = sample;
                        }
                        for slot in data.iter_mut().skip(n) {
                            *slot = 0.0;
                        }
                    },
                    err_fn,
                    None,
                )
                .ok()?;
            stream.play().ok()?;
            Some((stream, device_rate, channels))
        });
        let (stream, device_rate, channels) = match opened {
            Some((stream, rate, ch)) => (Some(stream), rate, ch),
            None => (None, output_rate_hz, 1),
        };
        let max_queue = (device_rate as usize).saturating_mul(channels).saturating_mul(2);
        CpalPcmSink {
            recorder,
            queue,
            bridge: Mutex::new(RateBridge::new(pipeline_rate_hz, device_rate, channels)),
            max_queue: max_queue.max(1024),
            _stream: stream,
        }
    }
}

impl PcmSink for CpalPcmSink {
    fn write(&self, pcm: &[u8]) {
        self.recorder.write(pcm);
        let converted = self.bridge.lock().unwrap().process_l16_mono(pcm);
        if converted.is_empty() {
            return;
        }
        let mut q = self.queue.lock().unwrap();
        q.extend_from_slice(&converted);
        if q.len() > self.max_queue {
            let drop = q.len() - self.max_queue;
            q.drain(..drop);
        }
    }
}

pub fn supported_output_rate_ranges(device: &cpal::Device) -> Vec<(u32, u32)> {
    match device.supported_output_configs() {
        Ok(cfgs) => cfgs
            .map(|cfg| (cfg.min_sample_rate(), cfg.max_sample_rate()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// A2DP catalog intersected with what this named OS output actually advertises.
pub fn supported_bluetooth_rates_for_name(name: &str) -> Vec<u32> {
    if looks_like_a2dp_sink(name) {
        return BLUETOOTH_RATES_HZ.to_vec();
    }
    let host = cpal::default_host();
    let ranges = host
        .output_devices()
        .ok()
        .and_then(|devices| devices.into_iter().find(|d| d.to_string() == name))
        .map(|device| supported_output_rate_ranges(&device))
        .unwrap_or_default();
    rates::intersect_catalog(&ranges, BLUETOOTH_RATES_HZ)
}

/// Pulse/BlueZ sink: `pacat` named device. Otherwise cpal by name.
pub fn pcm_sink_for_device(
    id: &str,
    name: &str,
    pipeline_hz: u32,
    output_hz: u32,
) -> Arc<dyn PcmSink> {
    if looks_like_a2dp_sink(id) || looks_like_a2dp_sink(name) {
        Arc::new(PacatPcmSink::new(id, pipeline_hz, output_hz))
    } else {
        Arc::new(CpalPcmSink::for_output_named(name, pipeline_hz, output_hz))
    }
}

/// Writes bridged L16 to a Pulse sink by name (`bluez_output.*`).
pub struct PacatPcmSink {
    recorder: RecordingPcmSink,
    bridge: Mutex<RateBridge>,
    stdin: Mutex<Option<std::process::ChildStdin>>,
    _child: Mutex<Option<std::process::Child>>,
}

impl PacatPcmSink {
    pub fn new(device: &str, pipeline_hz: u32, output_hz: u32) -> Self {
        let mut child = std::process::Command::new("pacat")
            .args([
                "--playback",
                "--raw",
                "--format=s16le",
                &format!("--rate={output_hz}"),
                "--channels=2",
                "--latency-msec=50",
                &format!("--device={device}"),
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();
        let stdin = child.as_mut().and_then(|c| c.stdin.take());
        PacatPcmSink {
            recorder: RecordingPcmSink::default(),
            bridge: Mutex::new(RateBridge::new(pipeline_hz, output_hz, 2)),
            stdin: Mutex::new(stdin),
            _child: Mutex::new(child),
        }
    }
}

impl PcmSink for PacatPcmSink {
    fn write(&self, pcm: &[u8]) {
        self.recorder.write(pcm);
        let bytes = self.bridge.lock().unwrap().process_l16_mono_to_l16(pcm);
        if bytes.is_empty() {
            return;
        }
        if let Some(stdin) = self.stdin.lock().unwrap().as_mut() {
            use std::io::Write;
            let _ = stdin.write_all(&bytes);
            let _ = stdin.flush();
        }
    }
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

    fn connect(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        let device = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| SenderError("bluetooth device not found".into()))?;
        if !device.paired {
            return Err(SenderError("bluetooth device not paired".into()));
        }
        for d in devices.iter_mut() {
            d.connected = d.id == id;
        }
        Ok(())
    }

    fn disconnect(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(device) = devices.iter_mut().find(|d| d.id == id) {
            device.connected = false;
        }
        Ok(())
    }

    fn set_volume(&self, _id: &str, _volume: u8) -> Result<(), SenderError> {
        Ok(())
    }
}

/// Production adapter: A2DP sinks only (BlueZ/Pulse `bluez_output.*`, or a
/// cpal device whose name actually says bluetooth). HDMI/analog are not Bluetooth.
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
                    devices.push(BluetoothDevice {
                        id,
                        name: display,
                        paired: true,
                        connected: false,
                    });
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
            devices.push(BluetoothDevice {
                id,
                name: display,
                paired: true,
                connected: false,
            });
        }
    }
    devices
}

fn pulse_a2dp_sinks() -> Vec<BluetoothDevice> {
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
        let pulse = pulse_a2dp_sinks();
        if !pulse.is_empty() || cfg!(target_os = "linux") {
            return pulse;
        }
        let mut devices = Vec::new();
        if let Ok(cpal_devices) = cpal::default_host().output_devices() {
            for d in cpal_devices {
                let name = d.to_string();
                if looks_like_a2dp_sink(&name) {
                    devices.push(BluetoothDevice {
                        id: name.clone(),
                        name,
                        paired: true,
                        connected: false,
                    });
                }
            }
        }
        devices
    }

    fn pair(&self, id: &str) -> Result<(), SenderError> {
        if self.list().iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(SenderError("bluetooth sink not found".into()))
        }
    }

    fn connect(&self, id: &str) -> Result<(), SenderError> {
        self.pair(id)?;
        crate::pipeline::local_sink::pin_default_sink();
        let _ = std::process::Command::new("pactl")
            .args(["set-sink-mute", id, "0"])
            .status();
        // PipeWire often makes a newly connected A2DP headset the default
        // sink. Put the user's DSP/analog default back; we play to Mini
        // with `pacat --device=` only.
        crate::pipeline::local_sink::restore_default_sink();
        Ok(())
    }

    fn disconnect(&self, _id: &str) -> Result<(), SenderError> {
        Ok(())
    }

    fn set_volume(&self, id: &str, volume: u8) -> Result<(), SenderError> {
        let pct = u32::from(volume.min(100));
        let _ = std::process::Command::new("pactl")
            .args(["set-sink-volume", id, &format!("{pct}%")])
            .status();
        let _ = std::process::Command::new("pactl")
            .args(["set-sink-mute", id, "0"])
            .status();
        Ok(())
    }
}

pub struct BluetoothSender {
    id: String,
    name: String,
    adapter: Arc<dyn BluetoothAdapter>,
    audio_tx: broadcast::Sender<Bytes>,
    sink: Arc<dyn PcmSink>,
    stop: Arc<AtomicBool>,
    pump: Option<tokio::task::JoinHandle<()>>,
}

impl BluetoothSender {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        adapter: Arc<dyn BluetoothAdapter>,
        audio_tx: broadcast::Sender<Bytes>,
        sink: Arc<dyn PcmSink>,
    ) -> Self {
        BluetoothSender {
            id: id.into(),
            name: name.into(),
            adapter,
            audio_tx,
            sink,
            stop: Arc::new(AtomicBool::new(false)),
            pump: None,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for BluetoothSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.adapter.connect(&self.id)?;
        self.stop.store(false, Ordering::Relaxed);
        let mut rx = self.audio_tx.subscribe();
        let sink = self.sink.clone();
        let stop = self.stop.clone();
        self.pump = Some(tokio::spawn(async move {
            while !stop.load(Ordering::Relaxed) {
                match rx.recv().await {
                    Ok(chunk) => sink.write(&chunk),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.pump.take() {
            handle.abort();
        }
        self.adapter.disconnect(&self.id)
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.adapter.set_volume(&self.id, volume)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn transport(&self) -> &'static str {
        "bluetooth"
    }
}
