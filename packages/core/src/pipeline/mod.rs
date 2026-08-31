use crate::api::ws::WsEvent;
use crate::dsp::eq::GraphicEq;
use crate::dsp::resample::MonoResampler;
use bytes::Bytes;
use ringbuf::{traits::*, HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

pub fn spawn_processing_task(
    mut consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<Bytes>,
    ws_tx: broadcast::Sender<WsEvent>,
) -> ProcessingTaskHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let join_handle = std::thread::spawn(move || {
        let mut eq = GraphicEq::new(input_rate_hz as f32);
        let mut resampler = (input_rate_hz != output_rate_hz)
            .then(|| MonoResampler::new(input_rate_hz, output_rate_hz));
        let chunk_frames = resampler
            .as_ref()
            .map(MonoResampler::input_frames_next)
            .unwrap_or(1024);
        let mut chunk = vec![0.0f32; chunk_frames];
        let mut resampled = Vec::new();

        while !stop_flag_thread.load(Ordering::Relaxed) {
            let mut filled = 0;
            while filled < chunk_frames {
                if stop_flag_thread.load(Ordering::Relaxed) {
                    return;
                }
                filled += consumer.pop_slice(&mut chunk[filled..]);
                if filled < chunk_frames {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }

            let audio_receivers = audio_tx.receiver_count();
            let meter_receivers = ws_tx.receiver_count();
            if audio_receivers == 0 && meter_receivers == 0 {
                continue;
            }

            eq.set_gains_db(*eq_gains_db.lock().unwrap());
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
    });

    ProcessingTaskHandle {
        stop_flag,
        join_handle: Some(join_handle),
    }
}

pub mod capture;
pub mod local_sink;

pub struct CaptureHandle {
    _stream: Option<cpal::Stream>,
    pulse: Option<capture::PulseMonitorCapture>,
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
        self.processing.stop();
    }
}
