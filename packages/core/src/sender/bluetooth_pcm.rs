//! Playback adapters behind the A2DP sink lifecycle. Endpoint kinds come from
//! platform discovery, never from the speaker's display name.
use crate::dsp::bridge::RateBridge;
use crate::dsp::channel::{ChannelPlan, Destination, SinkConstruction};
use crate::dsp::rates::{self, BLUETOOTH_RATES_HZ};
use crate::pipeline::push_frames;
use crate::sender::bluetooth::BluetoothPlaybackConfig;
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
/// `open` builds the sink's channel fit from the config's channel mode and
/// destination report (roadmap A4/A6), and refuses a stereo request for a
/// mono device rather than silently losing a channel.
pub trait PcmOutput: Send + Sync {
    fn open(
        &self,
        endpoint: &BluetoothEndpoint,
        config: &BluetoothPlaybackConfig,
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
        config: &BluetoothPlaybackConfig,
    ) -> Result<OpenedPcmSink, SenderError> {
        Ok(OpenedPcmSink {
            sink: self.clone(),
            format: OutputFormat {
                sample_rate_hz: rates::snap_rate(config.output_hz, BLUETOOTH_RATES_HZ),
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
        config: &BluetoothPlaybackConfig,
    ) -> Result<OpenedPcmSink, SenderError> {
        match endpoint {
            BluetoothEndpoint::Pulse(name) => {
                let sample_rate_hz = rates::snap_rate(config.output_hz, BLUETOOTH_RATES_HZ);
                Ok(OpenedPcmSink {
                    sink: Arc::new(PacatPcmSink::new(name, config, sample_rate_hz)?),
                    format: OutputFormat {
                        sample_rate_hz,
                        supported_hz: BLUETOOTH_RATES_HZ.to_vec(),
                    },
                })
            }
            BluetoothEndpoint::Native(name) => CpalPcmSink::open(name, config),
        }
    }
}

struct CpalPcmSink {
    producer: Mutex<HeapProd<f32>>,
    bridge: Mutex<RateBridge>,
    volume: Arc<AtomicU8>,
    stream: Mutex<Option<cpal::Stream>>,
}

impl CpalPcmSink {
    fn open(name: &str, playback: &BluetoothPlaybackConfig) -> Result<OpenedPcmSink, SenderError> {
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
        let sample_rate_hz = rates::snap_rate(playback.output_hz, &supported_hz);
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
        // The OS device dictates this stream's layout; the plan is resolved
        // against its honest report, so `stereo` on a mono endpoint is
        // rejected here with an explanation instead of losing a channel.
        let plan = SinkConstruction::CpalPcm
            .plan(
                playback.channel_mode,
                Destination::reported(name, config.channels),
            )
            .map_err(|error| SenderError::ChannelMode(error.message()))?;
        let queue_capacity = (sample_rate_hz as usize)
            .saturating_mul(plan.channels())
            .saturating_mul(2)
            .max(1024);
        let (producer, consumer) = HeapRb::<f32>::new(queue_capacity).split();
        let volume = Arc::new(AtomicU8::new(100));
        let stream_channels = plan.channels();
        let stream = match sample_format {
            SampleFormat::I8 => build_output_stream::<i8>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::I24 => build_output_stream::<I24>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::I64 => build_output_stream::<i64>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::U8 => build_output_stream::<u8>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::U24 => build_output_stream::<U24>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::U32 => build_output_stream::<u32>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::U64 => build_output_stream::<u64>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
            SampleFormat::F64 => build_output_stream::<f64>(
                &device,
                config,
                consumer,
                volume.clone(),
                stream_channels,
            )?,
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
                bridge: Mutex::new(RateBridge::new(playback.pipeline_hz, sample_rate_hz, &plan)),
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
        let (converted, channels) = {
            let mut bridge = self.bridge.lock();
            (bridge.process_l16(pcm), bridge.channels())
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
/// whole `stream_channels`-sample frames are taken; the rest of the buffer
/// is silence, so an underrun can never shift left into the right slot. A
/// mono stream (the user's explicit mono downmix) is spread across every
/// device channel at this hardware boundary: the pipeline stream itself is
/// genuinely mono, the device buffer the OS demands is not.
fn fill_output<T>(
    data: &mut [T],
    consumer: &mut ringbuf::HeapCons<f32>,
    stream_channels: usize,
    device_channels: usize,
    gain: f32,
) where
    T: Sample + FromSample<f32>,
{
    let stream_channels = stream_channels.max(1);
    let device_channels = device_channels.max(1);
    let frames = data.len() / device_channels;
    let available = consumer.occupied_len();
    let whole_frames = (available / stream_channels).min(frames);
    for frame in 0..frames {
        let mut last = 0.0f32;
        for slot in 0..device_channels {
            let sample = if frame >= whole_frames {
                0.0
            } else if slot < stream_channels {
                last = consumer.try_pop().unwrap_or(0.0);
                last
            } else if stream_channels == 1 {
                last
            } else {
                0.0
            };
            data[frame * device_channels + slot] = T::from_sample(sample * gain);
        }
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: ringbuf::HeapCons<f32>,
    volume: Arc<AtomicU8>,
    stream_channels: usize,
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
                fill_output(data, &mut consumer, stream_channels, channels, gain);
            },
            |e| eprintln!("bluetooth audio output error: {e}"),
            None,
        )
        .map_err(|e| SenderError::internal(format!("could not open audio output: {e}")))
}

struct PacatPcmSink {
    bridge: Mutex<RateBridge>,
    stdin: Mutex<std::process::ChildStdin>,
    child: Mutex<std::process::Child>,
    volume: AtomicU8,
}

/// The `pacat` invocation for one playback. `--channels` comes from the
/// justified channel plan, never a caller-spelled number: a user's mono
/// mode becomes a genuine mono stream (never a duplicated pair), and `auto`
/// keeps the historical stereo wire exactly (roadmap A4/A6).
fn pacat_args(device: &str, output_hz: u32, plan: &ChannelPlan) -> Vec<String> {
    vec![
        "--playback".into(),
        "--raw".into(),
        "--format=s16le".into(),
        format!("--rate={output_hz}"),
        format!("--channels={}", plan.channels().max(1)),
        "--latency-msec=50".into(),
        format!("--device={device}"),
    ]
}

impl PacatPcmSink {
    fn new(
        device: &str,
        playback: &BluetoothPlaybackConfig,
        output_hz: u32,
    ) -> Result<Self, SenderError> {
        let plan = SinkConstruction::PulsePcm
            .plan(playback.channel_mode, playback.destination.clone())
            .map_err(|error| SenderError::ChannelMode(error.message()))?;
        let mut child = std::process::Command::new("pacat")
            .args(pacat_args(device, output_hz, &plan))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| SenderError::internal(format!("could not start pacat: {e}")))?;
        let stdin = child.stdin.take().expect("piped pacat stdin");
        Ok(Self {
            bridge: Mutex::new(RateBridge::new(playback.pipeline_hz, output_hz, &plan)),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            volume: AtomicU8::new(100),
        })
    }
}
impl PcmSink for PacatPcmSink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        let mut bytes = self.bridge.lock().process_l16_to_l16(pcm);
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
    use crate::dsp::channel::{ChannelMode, ChannelPlan};
    use std::io::Write;
    use std::sync::mpsc;
    use std::time::Duration;

    fn stereo_plan() -> ChannelPlan {
        SinkConstruction::CpalPcm.auto(Destination::stereo("test device"))
    }

    #[test]
    fn a_full_output_queue_drops_whole_frames() {
        use crate::dsp::bridge::f32_to_l16_le;
        use ringbuf::traits::Observer;
        let (producer, consumer) = HeapRb::<f32>::new(5).split();
        let sink = CpalPcmSink {
            producer: Mutex::new(producer),
            bridge: Mutex::new(RateBridge::new(44_100, 44_100, &stereo_plan())),
            volume: Arc::new(AtomicU8::new(100)),
            stream: Mutex::new(None),
        };
        sink.write(&f32_to_l16_le(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3]))
            .unwrap();
        assert_eq!(consumer.occupied_len(), 4, "a frame was split");
    }

    #[test]
    fn the_output_callback_never_plays_part_of_a_frame() {
        use ringbuf::traits::Producer;
        let (mut producer, mut consumer) = HeapRb::<f32>::new(16).split();
        // A frame still being written: its left sample has landed, its right has not.
        producer.push_slice(&[0.1, -0.1, 0.2]);
        let mut data = [9.0f32; 4];
        fill_output(&mut data, &mut consumer, 2, 2, 1.0);
        assert_eq!(data, [0.1, -0.1, 0.0, 0.0]);
        producer.push_slice(&[-0.2]);
        fill_output(&mut data, &mut consumer, 2, 2, 1.0);
        assert_eq!(data, [0.2, -0.2, 0.0, 0.0], "left and right swapped");
    }

    #[test]
    fn a_mono_stream_is_spread_across_the_device_channels_at_the_hardware_boundary() {
        use ringbuf::traits::Producer;
        let (mut producer, mut consumer) = HeapRb::<f32>::new(16).split();
        // The stream crossing the pipeline is genuinely mono (one sample per
        // frame); the stereo device buffer the OS demands gets the downmix
        // on both channels here and nowhere else.
        producer.push_slice(&[0.5, -0.5]);
        let mut data = [9.0f32; 4];
        fill_output(&mut data, &mut consumer, 1, 2, 1.0);
        assert_eq!(data, [0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn pacat_follows_the_channel_plan() {
        let mono = SinkConstruction::PulsePcm
            .plan(ChannelMode::Mono, Destination::stereo("headset"))
            .unwrap();
        assert!(pacat_args("sink", 44_100, &mono).contains(&"--channels=1".to_string()));
        let auto = SinkConstruction::PulsePcm.auto(Destination::stereo("headset"));
        assert!(pacat_args("sink", 44_100, &auto).contains(&"--channels=2".to_string()));
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
            bridge: Mutex::new(RateBridge::new(44_100, 44_100, &stereo_plan())),
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
