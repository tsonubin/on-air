use crate::dsp::bridge::RateBridge;
use crate::dsp::rates::{self, BLUETOOTH_RATES_HZ};
use crate::sender::{AudioSender, SenderError};
use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig, I24, U24};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapProd, HeapRb};
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
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError>;
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
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        if !pcm.is_empty() {
            self.chunks
                .lock()
                .map_err(|_| SenderError("recording PCM sink lock poisoned".into()))?
                .push(pcm.to_vec());
        }
        Ok(())
    }
}

/// cpal output device used as the OS sink once a Bluetooth speaker is the system output.
pub struct CpalPcmSink {
    producer: Mutex<HeapProd<f32>>,
    bridge: Mutex<RateBridge>,
    _stream: cpal::Stream,
}

impl CpalPcmSink {
    pub fn for_output_named(
        name: &str,
        pipeline_rate_hz: u32,
        output_rate_hz: u32,
    ) -> Result<Self, SenderError> {
        let host = cpal::default_host();
        let device = host
            .output_devices()
            .map_err(|error| SenderError(format!("could not list audio outputs: {error}")))?
            .find(|device| device.to_string() == name)
            .ok_or_else(|| SenderError(format!("audio output not found: {name}")))?;
        let default = device.default_output_config().map_err(|error| {
            SenderError(format!("could not read output configuration: {error}"))
        })?;
        let supported = device
            .supported_output_configs()
            .ok()
            .and_then(|configs| {
                configs
                    .filter(|config| {
                        output_rate_hz >= config.min_sample_rate()
                            && output_rate_hz <= config.max_sample_rate()
                    })
                    .min_by_key(|config| {
                        (
                            config.sample_format() != default.sample_format(),
                            config.channels().abs_diff(default.channels()),
                        )
                    })
                    .map(|config| config.with_sample_rate(output_rate_hz))
            })
            .unwrap_or(default);
        let sample_format = supported.sample_format();
        let config = supported.config();
        let device_rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let queue_capacity = (device_rate as usize)
            .saturating_mul(channels)
            .saturating_mul(2)
            .max(1024);
        let (producer, consumer) = HeapRb::<f32>::new(queue_capacity).split();
        let stream = match sample_format {
            SampleFormat::I8 => build_output_stream::<i8>(&device, config, consumer)?,
            SampleFormat::I16 => build_output_stream::<i16>(&device, config, consumer)?,
            SampleFormat::I24 => build_output_stream::<I24>(&device, config, consumer)?,
            SampleFormat::I32 => build_output_stream::<i32>(&device, config, consumer)?,
            SampleFormat::I64 => build_output_stream::<i64>(&device, config, consumer)?,
            SampleFormat::U8 => build_output_stream::<u8>(&device, config, consumer)?,
            SampleFormat::U16 => build_output_stream::<u16>(&device, config, consumer)?,
            SampleFormat::U24 => build_output_stream::<U24>(&device, config, consumer)?,
            SampleFormat::U32 => build_output_stream::<u32>(&device, config, consumer)?,
            SampleFormat::U64 => build_output_stream::<u64>(&device, config, consumer)?,
            SampleFormat::F32 => build_output_stream::<f32>(&device, config, consumer)?,
            SampleFormat::F64 => build_output_stream::<f64>(&device, config, consumer)?,
            other => {
                return Err(SenderError(format!(
                    "unsupported output sample format: {other:?}"
                )))
            }
        };
        stream
            .play()
            .map_err(|error| SenderError(format!("could not start audio output: {error}")))?;
        Ok(CpalPcmSink {
            producer: Mutex::new(producer),
            bridge: Mutex::new(RateBridge::new(pipeline_rate_hz, device_rate, channels)),
            _stream: stream,
        })
    }
}

impl PcmSink for CpalPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        let converted = self
            .bridge
            .lock()
            .map_err(|_| SenderError("sample-rate bridge lock poisoned".into()))?
            .process_l16_mono(pcm);
        if converted.is_empty() {
            return Ok(());
        }
        // The queue is deliberately bounded. If the device falls behind, drop
        // new samples instead of growing memory or accumulating stale latency.
        self.producer
            .lock()
            .map_err(|_| SenderError("PCM queue lock poisoned".into()))?
            .push_slice(&converted);
        Ok(())
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: ringbuf::HeapCons<f32>,
) -> Result<cpal::Stream, SenderError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for slot in data {
                    *slot = consumer
                        .try_pop()
                        .map(T::from_sample)
                        .unwrap_or(T::EQUILIBRIUM);
                }
            },
            |error| eprintln!("bluetooth audio output error: {error}"),
            None,
        )
        .map_err(|error| SenderError(format!("could not open audio output: {error}")))
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
) -> Result<Arc<dyn PcmSink>, SenderError> {
    if looks_like_a2dp_sink(id) || looks_like_a2dp_sink(name) {
        Ok(Arc::new(PacatPcmSink::new(id, pipeline_hz, output_hz)?))
    } else {
        Ok(Arc::new(CpalPcmSink::for_output_named(
            name,
            pipeline_hz,
            output_hz,
        )?))
    }
}

/// Writes bridged L16 to a Pulse sink by name (`bluez_output.*`).
pub struct PacatPcmSink {
    bridge: Mutex<RateBridge>,
    stdin: Mutex<std::process::ChildStdin>,
    child: Mutex<std::process::Child>,
}

impl PacatPcmSink {
    pub fn new(device: &str, pipeline_hz: u32, output_hz: u32) -> Result<Self, SenderError> {
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
            .map_err(|error| SenderError(format!("could not start pacat: {error}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SenderError("pacat stdin is unavailable".into()))?;
        Ok(PacatPcmSink {
            bridge: Mutex::new(RateBridge::new(pipeline_hz, output_hz, 2)),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
        })
    }
}

impl PcmSink for PacatPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        let bytes = self
            .bridge
            .lock()
            .map_err(|_| SenderError("sample-rate bridge lock poisoned".into()))?
            .process_l16_mono_to_l16(pcm);
        if bytes.is_empty() {
            return Ok(());
        }
        use std::io::Write;
        self.stdin
            .lock()
            .map_err(|_| SenderError("pacat stdin lock poisoned".into()))?
            .write_all(&bytes)
            .map_err(|error| SenderError(format!("pacat playback failed: {error}")))
    }
}

impl Drop for PacatPcmSink {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
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
    writer: Option<std::thread::JoinHandle<()>>,
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
            writer: None,
        }
    }
}

impl Drop for BluetoothSender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.pump.take() {
            handle.abort();
        }
        // The writer wakes at least every 100 ms and exits. Normal lifecycle
        // calls `stop`, which joins it and disconnects the adapter.
    }
}

#[async_trait::async_trait]
impl AudioSender for BluetoothSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        if self.pump.is_some() || self.writer.is_some() {
            return Err(SenderError("bluetooth sender is already running".into()));
        }
        let adapter = self.adapter.clone();
        let id = self.id.clone();
        tokio::task::spawn_blocking(move || adapter.connect(&id))
            .await
            .map_err(|error| SenderError(format!("bluetooth connect task failed: {error}")))??;
        self.stop.store(false, Ordering::Relaxed);
        let mut rx = self.audio_tx.subscribe();
        let (writer_tx, writer_rx) = sync_channel::<Bytes>(8);
        let sink = self.sink.clone();
        let writer_stop = self.stop.clone();
        self.writer = Some(std::thread::spawn(move || {
            while !writer_stop.load(Ordering::Relaxed) {
                match writer_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(chunk) => {
                        if let Err(error) = sink.write(&chunk) {
                            eprintln!("bluetooth audio sink stopped: {error}");
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
        let pump_stop = self.stop.clone();
        self.pump = Some(tokio::spawn(async move {
            while !pump_stop.load(Ordering::Relaxed) {
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.pump.take() {
            handle.abort();
        }
        if let Some(writer) = self.writer.take() {
            tokio::task::spawn_blocking(move || writer.join())
                .await
                .map_err(|error| SenderError(format!("bluetooth writer task failed: {error}")))?
                .map_err(|_| SenderError("bluetooth writer thread panicked".into()))?;
        }
        let adapter = self.adapter.clone();
        let id = self.id.clone();
        tokio::task::spawn_blocking(move || adapter.disconnect(&id))
            .await
            .map_err(|error| SenderError(format!("bluetooth disconnect task failed: {error}")))?
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        let adapter = self.adapter.clone();
        let id = self.id.clone();
        tokio::task::spawn_blocking(move || adapter.set_volume(&id, volume))
            .await
            .map_err(|error| SenderError(format!("bluetooth volume task failed: {error}")))?
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn transport(&self) -> &'static str {
        "bluetooth"
    }
}
