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
    let mut buf = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let sample_i16 = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample_i16.to_le_bytes());
    }
    Bytes::from(buf)
}

pub fn spawn_processing_task(
    mut consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<Bytes>,
) -> ProcessingTaskHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let join_handle = std::thread::spawn(move || {
        let mut eq = GraphicEq::new(input_rate_hz as f32);
        let mut resampler = MonoResampler::new(input_rate_hz, output_rate_hz);
        let chunk_frames = resampler.input_frames_next();
        let mut chunk = vec![0.0f32; chunk_frames];

        while !stop_flag_thread.load(Ordering::Relaxed) {
            let mut filled = 0;
            while filled < chunk_frames {
                if stop_flag_thread.load(Ordering::Relaxed) {
                    return;
                }
                filled += consumer.pop_slice(&mut chunk[filled..]);
                if filled < chunk_frames {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }

            eq.set_gains_db(*eq_gains_db.lock().unwrap());
            eq.process(&mut chunk);
            let resampled = resampler.process(&chunk);
            let pcm_bytes = f32_to_le_i16_bytes(&resampled);
            let _ = audio_tx.send(pcm_bytes); // no subscribers is fine
        }
    });

    ProcessingTaskHandle {
        stop_flag,
        join_handle: Some(join_handle),
    }
}
