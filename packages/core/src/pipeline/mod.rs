use crate::dsp::bridge::TpdfDither;
use crate::dsp::eq::GraphicEq;
use crate::dsp::resample::PcmResampler;
use crate::dsp::{DspQuality, PIPELINE_CHANNELS};
use crate::events::WsEvent;
use crate::timeline::{PcmChunk, FRAME_BYTES};
use bytes::Bytes;
use on_air_api::MeterLevel;
use parking_lot::Mutex;
use ringbuf::{traits::*, HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// A capture ring holding `capacity_frames` interleaved pipeline frames.
pub fn new_ring_buffer(capacity_frames: usize) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(capacity_frames * PIPELINE_CHANNELS).split()
}

/// Pushes the most whole `channels`-sample frames of `samples` that fit and
/// returns how many samples went in. An overrun drops whole frames, never
/// one channel of a frame, which would swap left and right from then on.
pub fn push_frames(producer: &mut HeapProd<f32>, samples: &[f32], channels: usize) -> usize {
    let fit = producer.vacant_len().min(samples.len());
    producer.push_slice(&samples[..fit - fit % channels.max(1)])
}

pub struct ProcessingTaskHandle {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl ProcessingTaskHandle {
    /// Blocking: joins the processing thread. From async code, call this
    /// inside `tokio::task::spawn_blocking`.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

/// The chunk payload handed to `audio_tx` receivers — and the **one**
/// allocation the processing thread makes per chunk in steady state (the
/// C7 allocation guard pins it): a chunk is owned by asynchronous
/// receivers (a radio-stream client, a speaker sink) for as long as they
/// like, so the bytes must be handed over whole, and the `Bytes` bus type
/// offers no reclamation hook — every handoff needs one fresh buffer
/// (`Bytes::from(Vec)` itself allocates nothing on top). The allocation is
/// one buffer of `chunk samples × 2` bytes per 1024-frame chunk, and
/// nothing else on the thread allocates at all.
fn f32_to_le_i16_bytes(samples: &[f32], dither: &mut TpdfDither) -> Bytes {
    let mut buf = Vec::with_capacity(samples.len() * 2);
    crate::dsp::bridge::f32_to_l16_le_into(samples, dither, &mut buf);
    Bytes::from(buf)
}

/// Called once, from the processing thread, if it panics. Receives the
/// panic message.
pub type ProcessingDeathHook = Box<dyn FnOnce(String) + Send>;

/// An extra stage run on every captured chunk (interleaved pipeline frames)
/// before the EQ. Test-only fault injection uses it to make the thread panic.
pub(crate) type ProcessingStage = Box<dyn FnMut(&mut [f32]) + Send>;

pub fn spawn_processing_task(
    consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<PcmChunk>,
    ws_tx: broadcast::Sender<WsEvent>,
    quality: DspQuality,
) -> ProcessingTaskHandle {
    spawn_processing_task_reporting(
        consumer,
        ProcessingRates {
            input_hz: input_rate_hz,
            output_hz: output_rate_hz,
        },
        ProcessingOutputs {
            eq_gains_db,
            audio_tx,
            ws_tx,
            dsp_quality: quality,
        },
        None,
        None,
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessingRates {
    pub input_hz: u32,
    pub output_hz: u32,
}

pub struct ProcessingOutputs {
    pub eq_gains_db: Arc<Mutex<[f32; 5]>>,
    pub audio_tx: broadcast::Sender<PcmChunk>,
    pub ws_tx: broadcast::Sender<WsEvent>,
    /// The DSP quality profile the processing task's resampler is built
    /// with. Read once at thread start; a settings change takes effect on
    /// the next capture restart.
    pub dsp_quality: DspQuality,
}

/// Like [`spawn_processing_task`], but a panic inside the thread is caught
/// and handed to `on_death` instead of vanishing with the thread.
pub(crate) fn spawn_processing_task_reporting(
    consumer: HeapCons<f32>,
    rates: ProcessingRates,
    outputs: ProcessingOutputs,
    stage: Option<ProcessingStage>,
    on_death: Option<ProcessingDeathHook>,
) -> ProcessingTaskHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let join_handle = std::thread::spawn(move || {
        let body = std::panic::AssertUnwindSafe(|| {
            process_until_stopped(consumer, rates, outputs, stage, &stop_flag_thread)
        });
        if let Err(panic) = std::panic::catch_unwind(body) {
            if let Some(on_death) = on_death {
                on_death(panic_message(panic.as_ref()));
            }
        }
    });

    ProcessingTaskHandle {
        stop_flag,
        join_handle: Some(join_handle),
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let detail = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    format!("audio processing stopped: {detail}")
}

/// Per-channel RMS and peak of one interleaved stereo chunk, one pass over
/// the samples and no allocation. The pipeline carries interleaved
/// `[left, right]` frames ([`PIPELINE_CHANNELS`]); a mono capture is
/// duplicated into both sides upstream, so the meter then reports the same
/// reading on both channels without a special case here.
fn stereo_levels(samples: &[f32]) -> (MeterLevel, MeterLevel) {
    let mut left_peak = 0.0f32;
    let mut right_peak = 0.0f32;
    let mut left_sq = 0.0f64;
    let mut right_sq = 0.0f64;
    let (frames, _) = samples.as_chunks::<PIPELINE_CHANNELS>();
    for frame in frames {
        let (left, right) = (frame[0], frame[1]);
        left_peak = left_peak.max(left.abs());
        right_peak = right_peak.max(right.abs());
        left_sq += f64::from(left) * f64::from(left);
        right_sq += f64::from(right) * f64::from(right);
    }
    let per_channel = frames.len().max(1) as f64;
    (
        MeterLevel {
            rms: (left_sq / per_channel).sqrt() as f32,
            peak: left_peak,
        },
        MeterLevel {
            rms: (right_sq / per_channel).sqrt() as f32,
            peak: right_peak,
        },
    )
}

fn process_until_stopped(
    mut consumer: HeapCons<f32>,
    rates: ProcessingRates,
    outputs: ProcessingOutputs,
    mut stage: Option<ProcessingStage>,
    stop_flag: &AtomicBool,
) {
    let ProcessingOutputs {
        eq_gains_db,
        audio_tx,
        ws_tx,
        dsp_quality,
    } = outputs;
    let mut eq = GraphicEq::new(rates.input_hz as f32, PIPELINE_CHANNELS);
    let mut resampler = (rates.input_hz != rates.output_hz).then(|| {
        PcmResampler::new(
            rates.input_hz,
            rates.output_hz,
            PIPELINE_CHANNELS,
            dsp_quality,
        )
    });
    let chunk_frames = resampler
        .as_ref()
        .map(PcmResampler::input_frames_next)
        .unwrap_or(1024);
    let mut chunk = vec![0.0f32; chunk_frames * PIPELINE_CHANNELS];
    let mut resampled = Vec::new();
    // One dither per capture, so parallel captures do not share noise.
    let mut dither = TpdfDither::for_stream(rates.input_hz, rates.output_hz, PIPELINE_CHANNELS);
    // Frames published so far: the bus timeline's frame index origin.
    let mut frames_emitted = 0u64;

    while !stop_flag.load(Ordering::Relaxed) {
        let mut filled = 0;
        while filled < chunk.len() {
            if stop_flag.load(Ordering::Relaxed) {
                return;
            }
            filled += consumer.pop_slice(&mut chunk[filled..]);
            if filled < chunk.len() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        if let Some(stage) = stage.as_mut() {
            stage(&mut chunk);
        }

        let audio_receivers = audio_tx.receiver_count();
        let meter_receivers = ws_tx.receiver_count();
        if audio_receivers == 0 && meter_receivers == 0 {
            continue;
        }

        eq.set_gains_db(*eq_gains_db.lock());
        eq.process(&mut chunk);

        if meter_receivers > 0 {
            let (left, right) = stereo_levels(&chunk);
            let _ = ws_tx.send(WsEvent::LevelMeter { left, right });
        }

        if audio_receivers > 0 {
            let pcm = match resampler.as_mut() {
                Some(resampler) => {
                    resampler.process_into(&chunk, &mut resampled);
                    f32_to_le_i16_bytes(&resampled, &mut dither)
                }
                None => f32_to_le_i16_bytes(&chunk, &mut dither),
            };
            // Publish on the timeline: contiguous frame indices, one
            // capture stamp, the same buffer the free-running path used.
            // Empty chunks (a resampler catch-up produced no frames) are
            // skipped so frame indices stay strictly contiguous.
            if !pcm.is_empty() {
                let frames = (pcm.len() / FRAME_BYTES) as u64;
                let _ = audio_tx.send(PcmChunk {
                    first_frame: frames_emitted,
                    captured_at: Instant::now(),
                    pcm,
                });
                frames_emitted += frames;
            }
        }
    }
}

pub mod capture;
pub mod input;
pub mod local_sink;
pub mod sources;

#[cfg(test)]
mod alloc_guard;

pub use input::{InputError, InputSession, InputSnapshot, LOOPBACK_INPUT};

pub struct CaptureHandle {
    _stream: Option<cpal::Stream>,
    pulse: Option<capture::PulseMonitorCapture>,
    on_stop: Option<Box<dyn FnOnce() + Send>>,
    processing: ProcessingTaskHandle,
    pub device_name: String,
}

impl CaptureHandle {
    pub fn cpal(
        stream: cpal::Stream,
        processing: ProcessingTaskHandle,
        device_name: String,
    ) -> Self {
        CaptureHandle {
            _stream: Some(stream),
            pulse: None,
            on_stop: None,
            processing,
            device_name,
        }
    }

    pub fn pulse(
        pulse: capture::PulseMonitorCapture,
        processing: ProcessingTaskHandle,
        device_name: String,
    ) -> Self {
        CaptureHandle {
            _stream: None,
            pulse: Some(pulse),
            on_stop: None,
            processing,
            device_name,
        }
    }

    /// A capture whose producer is fed by something the caller owns (the CD
    /// deck); only the processing thread needs stopping.
    pub fn processing_only(processing: ProcessingTaskHandle, device_name: String) -> Self {
        CaptureHandle {
            _stream: None,
            pulse: None,
            on_stop: None,
            processing,
            device_name,
        }
    }

    /// Blocking: stops the capture stream and joins the processing thread.
    /// From async code, call this inside `tokio::task::spawn_blocking`.
    pub fn stop(self) {
        drop(self._stream);
        if let Some(pulse) = self.pulse {
            pulse.stop();
        }
        if let Some(on_stop) = self.on_stop {
            on_stop();
        }
        self.processing.stop();
    }
}
