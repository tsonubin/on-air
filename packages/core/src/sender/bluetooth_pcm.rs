//! Playback adapters behind the A2DP sink lifecycle. Endpoint kinds come from
//! platform discovery, never from the speaker's display name.
use crate::dsp::bridge::RateBridge;
use crate::dsp::rates::{self, BLUETOOTH_RATES_HZ};
use crate::pipeline::push_frames;
use crate::sender::{OutputFormat, SenderError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig, I24, U24};
use parking_lot::Mutex;
use ringbuf::traits::{Consumer, Observer, Split};
use ringbuf::{HeapProd, HeapRb};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothEndpoint {
    Pulse(String),
    Native(String),
}

/// A live destination for pipeline PCM. Close must stop playback and unblock
/// concurrent writes without waiting for their locks. It must be idempotent.
pub trait PcmSink: Send + Sync {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError>;
    fn set_volume(&self, volume: u8) -> Result<(), SenderError>;
    fn close(&self) -> Result<(), SenderError>;
}

pub struct OpenedPcmSink {
    pub sink: Arc<dyn PcmSink>,
    pub format: OutputFormat,
}

impl Drop for OpenedPcmSink {
    fn drop(&mut self) {
        // Preparation can finish after its waiting task is canceled. Explicit
        // close is required even when a factory retains another Arc to the sink.
        if let Err(error) = self.sink.close() {
            eprintln!("could not close Bluetooth output: {error}");
        }
    }
}

/// Reopenable playback configuration: a fresh stream/queue/bridge per start.
pub trait PcmOutput: Send + Sync {
    fn open(
        &self,
        endpoint: &BluetoothEndpoint,
        pipeline_hz: u32,
        preferred_hz: u32,
        quality: crate::dsp::DspQuality,
    ) -> Result<OpenedPcmSink, SenderError>;
}

pub struct RecordingPcmSink {
    pub chunks: Mutex<Vec<Vec<u8>>>,
    volume: AtomicU8,
}

impl Default for RecordingPcmSink {
    fn default() -> Self {
        Self {
            chunks: Mutex::new(Vec::new()),
            volume: AtomicU8::new(100),
        }
    }
}

impl RecordingPcmSink {
    pub fn byte_count(&self) -> usize {
        self.chunks.lock().iter().map(|c| c.len()).sum()
    }
}

impl PcmSink for RecordingPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        if !pcm.is_empty() {
            let mut scaled = pcm.to_vec();
            scale_l16(&mut scaled, self.volume.load(Ordering::Acquire));
            self.chunks.lock().push(scaled);
        }
        Ok(())
    }
    fn set_volume(&self, volume: u8) -> Result<(), SenderError> {
        self.volume.store(volume.min(100), Ordering::Release);
        Ok(())
    }
    fn close(&self) -> Result<(), SenderError> {
        Ok(())
    }
}

impl PcmOutput for Arc<RecordingPcmSink> {
    fn open(
        &self,
        _: &BluetoothEndpoint,
        _: u32,
        preferred_hz: u32,
        _: crate::dsp::DspQuality,
    ) -> Result<OpenedPcmSink, SenderError> {
        Ok(OpenedPcmSink {
            sink: self.clone(),
            format: OutputFormat {
                sample_rate_hz: rates::snap_rate(preferred_hz, BLUETOOTH_RATES_HZ),
                supported_hz: BLUETOOTH_RATES_HZ.to_vec(),
            },
        })
    }
}

pub struct SystemPcmOutput;
impl PcmOutput for SystemPcmOutput {
    fn open(
        &self,
        endpoint: &BluetoothEndpoint,
        pipeline_hz: u32,
        preferred_hz: u32,
        quality: crate::dsp::DspQuality,
    ) -> Result<OpenedPcmSink, SenderError> {
        match endpoint {
            BluetoothEndpoint::Pulse(name) => {
                let sample_rate_hz = rates::snap_rate(preferred_hz, BLUETOOTH_RATES_HZ);
                Ok(OpenedPcmSink {
                    sink: Arc::new(PacatPcmSink::new(name, pipeline_hz, sample_rate_hz, quality)?),
                    format: OutputFormat {
                        sample_rate_hz,
                        supported_hz: BLUETOOTH_RATES_HZ.to_vec(),
                    },
                })
            }
            BluetoothEndpoint::Native(name) => {
                CpalPcmSink::open(name, pipeline_hz, preferred_hz, quality)
            }
        }
    }
}

struct CpalPcmSink {
    producer: Mutex<HeapProd<f32>>,
    bridge: Mutex<RateBridge>,
    /// The bridge's output scratch, so `write` allocates nothing per chunk
    /// in steady state.
    converted: Mutex<Vec<f32>>,
    volume: Arc<AtomicU8>,
    stream: Mutex<Option<cpal::Stream>>,
}

impl CpalPcmSink {
    fn open(
        name: &str,
        pipeline_hz: u32,
        preferred_hz: u32,
        quality: crate::dsp::DspQuality,
    ) -> Result<OpenedPcmSink, SenderError> {
        let device = cpal::default_host()
            .output_devices()
            .map_err(|e| SenderError::internal(format!("could not list audio outputs: {e}")))?
            .find(|device| device.to_string() == name)
            .ok_or_else(|| {
                SenderError::DeviceNotFound(format!("audio output not found: {name}"))
            })?;
        let default = device.default_output_config().map_err(|e| {
            SenderError::internal(format!("could not read output configuration: {e}"))
        })?;
        let configs: Vec<_> = device
            .supported_output_configs()
            .map_err(|e| {
                SenderError::internal(format!("could not read supported output rates: {e}"))
            })?
            .collect();
        let supported_hz: Vec<_> = BLUETOOTH_RATES_HZ
            .iter()
            .copied()
            .filter(|hz| {
                configs.iter().any(|config| {
                    *hz >= config.min_sample_rate() && *hz <= config.max_sample_rate()
                })
            })
            .collect();
        if supported_hz.is_empty() {
            return Err(SenderError::not_ready(
                "Bluetooth output has no supported A2DP sample rate",
            ));
        }
        let sample_rate_hz = rates::snap_rate(preferred_hz, &supported_hz);
        let supported = configs
            .into_iter()
            .filter(|config| {
                sample_rate_hz >= config.min_sample_rate()
                    && sample_rate_hz <= config.max_sample_rate()
            })
            .min_by_key(|config| {
                (
                    config.sample_format() != default.sample_format(),
                    config.channels().abs_diff(default.channels()),
                )
            })
            .ok_or_else(|| {
                SenderError::not_ready("Bluetooth output has no supported A2DP sample rate")
            })?
            .with_sample_rate(sample_rate_hz);
        let sample_format = supported.sample_format();
        let config = supported.config();
        let channels = usize::from(config.channels);
        let queue_capacity = (sample_rate_hz as usize)
            .saturating_mul(channels)
            .saturating_mul(2)
            .max(1024);
        let (producer, consumer) = HeapRb::<f32>::new(queue_capacity).split();
        let volume = Arc::new(AtomicU8::new(100));
        let stream = match sample_format {
            SampleFormat::I8 => {
                build_output_stream::<i8>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::I16 => {
                build_output_stream::<i16>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::I24 => {
                build_output_stream::<I24>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::I32 => {
                build_output_stream::<i32>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::I64 => {
                build_output_stream::<i64>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::U8 => {
                build_output_stream::<u8>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::U16 => {
                build_output_stream::<u16>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::U24 => {
                build_output_stream::<U24>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::U32 => {
                build_output_stream::<u32>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::U64 => {
                build_output_stream::<u64>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::F32 => {
                build_output_stream::<f32>(&device, config, consumer, volume.clone())?
            }
            SampleFormat::F64 => {
                build_output_stream::<f64>(&device, config, consumer, volume.clone())?
            }
            other => {
                return Err(SenderError::internal(format!(
                    "unsupported output sample format: {other:?}"
                )))
            }
        };
        stream
            .play()
            .map_err(|e| SenderError::internal(format!("could not start audio output: {e}")))?;
        Ok(OpenedPcmSink {
            sink: Arc::new(Self {
                producer: Mutex::new(producer),
                bridge: Mutex::new(RateBridge::new(
                    pipeline_hz,
                    sample_rate_hz,
                    channels,
                    quality,
                )),
                converted: Mutex::new(Vec::new()),
                volume,
                stream: Mutex::new(Some(stream)),
            }),
            format: OutputFormat {
                sample_rate_hz,
                supported_hz,
            },
        })
    }
}

impl PcmSink for CpalPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        let mut converted = self.converted.lock();
        let channels = {
            let mut bridge = self.bridge.lock();
            bridge.process_l16_into(pcm, &mut converted);
            bridge.channels()
        };
        // Bound latency and memory: a slow device drops new frames, whole.
        push_frames(&mut self.producer.lock(), &converted, channels);
        Ok(())
    }
    fn set_volume(&self, volume: u8) -> Result<(), SenderError> {
        self.volume.store(volume.min(100), Ordering::Release);
        Ok(())
    }
    fn close(&self) -> Result<(), SenderError> {
        // Dropping the stream stops callbacks even if another Arc holds this sink.
        self.stream.lock().take();
        Ok(())
    }
}

/// Fills one output callback buffer from the queue, scaled by `gain`. Only
/// whole `channels`-sample frames are taken; the rest of the buffer is
/// silence, so an underrun can never shift left into the right slot.
fn fill_output<T>(data: &mut [T], consumer: &mut ringbuf::HeapCons<f32>, channels: usize, gain: f32)
where
    T: Sample + FromSample<f32>,
{
    let available = consumer.occupied_len().min(data.len());
    let take = available - available % channels.max(1);
    for (i, slot) in data.iter_mut().enumerate() {
        let sample = if i < take {
            consumer.try_pop().unwrap_or(0.0)
        } else {
            0.0
        };
        *slot = T::from_sample(sample * gain);
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: ringbuf::HeapCons<f32>,
    volume: Arc<AtomicU8>,
) -> Result<cpal::Stream, SenderError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // Apply gain at playback, including samples queued before a mute request.
                let gain = f32::from(volume.load(Ordering::Acquire)) / 100.0;
                fill_output(data, &mut consumer, channels, gain);
            },
            |e| eprintln!("bluetooth audio output error: {e}"),
            None,
        )
        .map_err(|e| SenderError::internal(format!("could not open audio output: {e}")))
}

struct PacatPcmSink {
    bridge: Mutex<RateBridge>,
    /// The bridge's L16 output scratch, so `write` allocates nothing per
    /// chunk in steady state.
    bytes: Mutex<Vec<u8>>,
    stdin: Mutex<std::process::ChildStdin>,
    child: Mutex<std::process::Child>,
    volume: AtomicU8,
}

impl PacatPcmSink {
    fn new(
        device: &str,
        pipeline_hz: u32,
        output_hz: u32,
        quality: crate::dsp::DspQuality,
    ) -> Result<Self, SenderError> {
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
            .map_err(|e| SenderError::internal(format!("could not start pacat: {e}")))?;
        let stdin = child.stdin.take().expect("piped pacat stdin");
        Ok(Self {
            bridge: Mutex::new(RateBridge::new(pipeline_hz, output_hz, 2, quality)),
            bytes: Mutex::new(Vec::new()),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            volume: AtomicU8::new(100),
        })
    }
}
impl PcmSink for PacatPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        let mut bytes = self.bytes.lock();
        self.bridge
            .lock()
            .process_l16_to_l16_into(pcm, &mut bytes);
        scale_l16(&mut bytes, self.volume.load(Ordering::Acquire));
        use std::io::Write;
        self.stdin
            .lock()
            .write_all(&bytes)
            .map_err(|e| SenderError::internal(format!("pacat playback failed: {e}")))
    }
    fn set_volume(&self, volume: u8) -> Result<(), SenderError> {
        self.volume.store(volume.min(100), Ordering::Release);
        Ok(())
    }
    fn close(&self) -> Result<(), SenderError> {
        // Never take stdin here: a writer may be blocked while holding it.
        let mut child = self.child.lock();
        if child
            .try_wait()
            .map_err(|e| SenderError::internal(format!("could not inspect pacat: {e}")))?
            .is_none()
        {
            child
                .kill()
                .map_err(|e| SenderError::internal(format!("could not stop pacat: {e}")))?;
        }
        child
            .wait()
            .map_err(|e| SenderError::internal(format!("could not reap pacat: {e}")))?;
        Ok(())
    }
}
impl Drop for PacatPcmSink {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn scale_l16(pcm: &mut [u8], volume: u8) {
    if volume == 100 {
        return;
    }
    for sample in pcm.as_chunks_mut::<2>().0 {
        let value = i32::from(i16::from_le_bytes(*sample));
        *sample = ((value * i32::from(volume) / 100) as i16).to_le_bytes();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc;
    use std::time::Duration;

    fn l16(samples: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        crate::dsp::bridge::f32_to_l16_le_into(samples, &mut out);
        out
    }

    #[test]
    fn a_full_output_queue_drops_whole_frames() {
        use ringbuf::traits::Observer;
        let (producer, consumer) = HeapRb::<f32>::new(5).split();
        let sink = CpalPcmSink {
            producer: Mutex::new(producer),
            bridge: Mutex::new(RateBridge::new(
                44_100,
                44_100,
                2,
                crate::dsp::DspQuality::default(),
            )),
            converted: Mutex::new(Vec::new()),
            volume: Arc::new(AtomicU8::new(100)),
            stream: Mutex::new(None),
        };
        sink.write(&l16(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3])).unwrap();
        assert_eq!(consumer.occupied_len(), 4, "a frame was split");
    }

    #[test]
    fn the_output_callback_never_plays_part_of_a_frame() {
        use ringbuf::traits::Producer;
        let (mut producer, mut consumer) = HeapRb::<f32>::new(16).split();
        // A frame still being written: its left sample has landed, its right has not.
        producer.push_slice(&[0.1, -0.1, 0.2]);
        let mut data = [9.0f32; 4];
        fill_output(&mut data, &mut consumer, 2, 1.0);
        assert_eq!(data, [0.1, -0.1, 0.0, 0.0]);
        producer.push_slice(&[-0.2]);
        fill_output(&mut data, &mut consumer, 2, 1.0);
        assert_eq!(data, [0.2, -0.2, 0.0, 0.0], "left and right swapped");
    }

    #[test]
    fn closing_child_interrupts_a_full_pipe_without_taking_the_stdin_lock() {
        // Use a child that never reads stdin, exercising the production pipe and
        // termination path without requiring PulseAudio or a physical speaker.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let sink = Arc::new(PacatPcmSink {
            bridge: Mutex::new(RateBridge::new(
                44_100,
                44_100,
                2,
                crate::dsp::DspQuality::default(),
            )),
            bytes: Mutex::new(Vec::new()),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            volume: AtomicU8::new(100),
        });
        let (entered_tx, entered_rx) = mpsc::channel();
        let writer_sink = sink.clone();
        let writer = std::thread::spawn(move || {
            let mut stdin = writer_sink.stdin.lock();
            entered_tx.send(()).unwrap();
            stdin.write_all(&vec![0; 1024 * 1024])
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let (closed_tx, closed_rx) = mpsc::channel();
        let closing_sink = sink.clone();
        let closer = std::thread::spawn(move || {
            closed_tx.send(closing_sink.close()).unwrap();
        });
        let closed = closed_rx.recv_timeout(Duration::from_secs(2));
        // A broken close implementation must fail the test without leaving a
        // blocked thread or a sleeping subprocess in the test runner.
        if closed.is_err() {
            let _ = sink.child.lock().kill();
        }
        assert!(writer.join().unwrap().is_err());
        closer.join().unwrap();
        closed
            .expect("close waited for the writer's stdin lock")
            .unwrap();
        assert!(sink.child.lock().try_wait().unwrap().is_some());
        sink.close().unwrap();
    }
}
