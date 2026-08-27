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
    _stream: Option<cpal::Stream>,
}

impl CpalPcmSink {
    pub fn for_output_named(name: &str) -> Self {
        let recorder = RecordingPcmSink::default();
        let queue = Arc::new(Mutex::new(Vec::<f32>::new()));
        let cb_queue = queue.clone();
        let host = cpal::default_host();
        let stream = host.output_devices().ok().and_then(|devices| {
            let device = devices.into_iter().find(|d| d.to_string() == name)?;
            let supported = device.default_output_config().ok()?;
            let config = supported.config();
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
            Some(stream)
        });
        CpalPcmSink {
            recorder,
            queue,
            _stream: stream,
        }
    }
}

impl PcmSink for CpalPcmSink {
    fn write(&self, pcm: &[u8]) {
        self.recorder.write(pcm);
        let mut q = self.queue.lock().unwrap();
        for chunk in pcm.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            q.push(sample as f32 / i16::MAX as f32);
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

/// Production adapter: OS output devices are the Bluetooth/system sinks we can route PCM into.
pub struct SystemBluetoothAdapter;

impl SystemBluetoothAdapter {
    pub fn from_host() -> Self {
        SystemBluetoothAdapter
    }
}

impl BluetoothAdapter for SystemBluetoothAdapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        let host = cpal::default_host();
        match host.output_devices() {
            Ok(devices) => devices
                .map(|d| {
                    let name = d.to_string();
                    BluetoothDevice {
                        id: name.clone(),
                        name,
                        paired: true,
                        connected: false,
                    }
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn pair(&self, id: &str) -> Result<(), SenderError> {
        if self.list().iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(SenderError("bluetooth sink not found".into()))
        }
    }

    fn connect(&self, id: &str) -> Result<(), SenderError> {
        self.pair(id)
    }

    fn disconnect(&self, _id: &str) -> Result<(), SenderError> {
        Ok(())
    }

    fn set_volume(&self, _id: &str, _volume: u8) -> Result<(), SenderError> {
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
}
