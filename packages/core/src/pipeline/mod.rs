use crate::dsp::eq::GraphicEq;
use crate::dsp::resample::MonoResampler;
use crate::events::WsEvent;
use bytes::Bytes;
use parking_lot::Mutex;
use ringbuf::{traits::*, HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

pub fn new_ring_buffer(capacity_frames: usize) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(capacity_frames).split()
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

fn f32_to_le_i16_bytes(samples: &[f32]) -> Bytes {
    Bytes::from(crate::dsp::bridge::f32_to_l16_le(samples))
}

/// Called once, from the processing thread, if it panics. Receives the
/// panic message.
pub type ProcessingDeathHook = Box<dyn FnOnce(String) + Send>;

/// An extra stage run on every captured chunk before the EQ. Test-only
/// fault injection uses it to make the thread panic.
pub(crate) type ProcessingStage = Box<dyn FnMut(&mut [f32]) + Send>;

pub fn spawn_processing_task(
    consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<Bytes>,
    ws_tx: broadcast::Sender<WsEvent>,
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
    pub audio_tx: broadcast::Sender<Bytes>,
    pub ws_tx: broadcast::Sender<WsEvent>,
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
    } = outputs;
    let mut eq = GraphicEq::new(rates.input_hz as f32);
    let mut resampler = (rates.input_hz != rates.output_hz)
        .then(|| MonoResampler::new(rates.input_hz, rates.output_hz));
    let chunk_frames = resampler
        .as_ref()
        .map(MonoResampler::input_frames_next)
        .unwrap_or(1024);
    let mut chunk = vec![0.0f32; chunk_frames];
    let mut resampled = Vec::new();

    while !stop_flag.load(Ordering::Relaxed) {
        let mut filled = 0;
        while filled < chunk_frames {
            if stop_flag.load(Ordering::Relaxed) {
                return;
            }
            filled += consumer.pop_slice(&mut chunk[filled..]);
            if filled < chunk_frames {
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
            let peak = chunk.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            let _ = ws_tx.send(WsEvent::LevelMeter { rms, peak });
        }

        if audio_receivers > 0 {
            let pcm_bytes = match resampler.as_mut() {
                Some(resampler) => {
                    resampler.process_into(&chunk, &mut resampled);
                    f32_to_le_i16_bytes(&resampled)
                }
                None => f32_to_le_i16_bytes(&chunk),
            };
            let _ = audio_tx.send(pcm_bytes);
        }
    }
}

pub mod capture;
pub mod input;
pub mod local_sink;

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

    pub fn with_cleanup(
        processing: ProcessingTaskHandle,
        device_name: String,
        on_stop: Box<dyn FnOnce() + Send>,
    ) -> Self {
        CaptureHandle {
            _stream: None,
            pulse: None,
            on_stop: Some(on_stop),
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
