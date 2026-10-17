//! A small, isolated run of the real DSP pipeline. No samples are sent to a
//! live output, and settings change only through the separate buffer action.
use crate::dsp::bridge::RateBridge;
use crate::dsp::PIPELINE_CHANNELS;
use crate::pipeline::{new_ring_buffer, spawn_processing_task};
use on_air_api::AudioBenchmark;
use parking_lot::Mutex;
use ringbuf::traits::Producer;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

pub const BUFFER_OPTIONS_MS: &[u32] = &[0, 50, 100, 200, 400];
const CHUNK_FRAMES: usize = 1024;
const SAFETY_FACTOR: f64 = 3.0;

fn p95(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() * 95).div_ceil(100).saturating_sub(1)]
}

/// Three times the observed worst round trip plus one capture quantum.
/// This is timing headroom, not a reservation of the host's CPU resources.
pub fn recommendation(worst_case_ms: f64, chunk_ms: f64) -> (u32, bool) {
    let budget = worst_case_ms * SAFETY_FACTOR + chunk_ms;
    let choice = BUFFER_OPTIONS_MS
        .iter()
        .copied()
        .skip(1)
        .find(|ms| f64::from(*ms) >= budget);
    (choice.unwrap_or(400), choice.is_some())
}

pub fn run(
    input_hz: u32,
    output_hz: u32,
    gains: [f32; 5],
    iterations: usize,
) -> Result<AudioBenchmark, String> {
    if input_hz == 0 || output_hz == 0 || iterations < 8 {
        return Err("invalid audio benchmark configuration".into());
    }
    let (mut producer, consumer) = new_ring_buffer(CHUNK_FRAMES * 8);
    let (audio_tx, mut receiver) = broadcast::channel(8);
    let (meter_tx, _meter_rx) = broadcast::channel(8);
    let processing = spawn_processing_task(
        consumer,
        input_hz,
        input_hz,
        Arc::new(Mutex::new(gains)),
        audio_tx,
        meter_tx,
    );
    let result = (|| {
        let samples: Vec<_> = (0..CHUNK_FRAMES)
            .flat_map(|i| [(i as f32 * 0.05).sin() * 0.3; PIPELINE_CHANNELS])
            .collect();
        let mut bridge = RateBridge::new(input_hz, output_hz, 2);
        let quantum = Duration::from_secs_f64(CHUNK_FRAMES as f64 / f64::from(input_hz));
        let started = Instant::now();
        let mut deadline = started;
        let mut timings = Vec::with_capacity(iterations);
        for index in 0..iterations + 4 {
            if index >= 12 && started.elapsed() >= Duration::from_secs(3) {
                break;
            }
            if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            let dispatched = Instant::now();
            let scheduler = dispatched.saturating_duration_since(deadline).as_secs_f64() * 1000.0;
            if producer.push_slice(&samples) != samples.len() {
                return Err(
                    "the benchmark pipeline could not keep up; reduce the sample rate and retry"
                        .into(),
                );
            }
            let pcm = loop {
                match receiver.try_recv() {
                    Ok(pcm) => break pcm,
                    Err(broadcast::error::TryRecvError::Empty)
                        if dispatched.elapsed() < Duration::from_secs(1) =>
                    {
                        std::thread::sleep(Duration::from_micros(500));
                    }
                    _ => {
                        return Err(
                            "the benchmark pipeline did not respond within one second".into()
                        )
                    }
                }
            };
            std::hint::black_box(bridge.process_l16(&pcm));
            let pipeline = dispatched.elapsed().as_secs_f64() * 1000.0;
            if index >= 4 {
                timings.push((pipeline, scheduler));
            }
            // Do not compound a temporary missed deadline into every later
            // sample. Each iteration probes a fresh scheduling interval.
            deadline = Instant::now() + quantum.saturating_sub(dispatched.elapsed());
        }
        let worst = timings.iter().map(|(p, s)| p + s).fold(0.0, f64::max);
        let (recommended, within_buffer_budget) =
            recommendation(worst, quantum.as_secs_f64() * 1000.0);
        let mut pipeline: Vec<_> = timings.iter().map(|(p, _)| *p).collect();
        let mut scheduler: Vec<_> = timings.iter().map(|(_, s)| *s).collect();
        Ok(AudioBenchmark {
            input_hz,
            output_hz,
            duration_ms: started.elapsed().as_millis() as u64,
            pipeline_p95_ms: p95(&mut pipeline),
            scheduler_p95_ms: p95(&mut scheduler),
            worst_case_ms: worst,
            recommended_buffer_ms: recommended,
            within_buffer_budget,
        })
    })();
    processing.stop();
    result
}
