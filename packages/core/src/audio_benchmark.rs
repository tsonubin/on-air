//! A small, isolated run of the real DSP pipeline. No samples are sent to a
//! live output, and settings change only through the separate buffer action.
use crate::dsp::bridge::RateBridge;
use crate::dsp::{DspQuality, PIPELINE_CHANNELS};
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
/// Steady-state probe chunks per profile when costing the rate bridge.
const COST_PROBES: usize = 16;

/// One audio check's full verdict: the wire report `POST
/// /api/audio/benchmark` returns, plus the measured per-profile DSP costs
/// the first-run tuning needs (`profile_costs_ms` is measurement detail,
/// not wire) and the recommendation both carry.
#[derive(Debug, Clone)]
pub struct BenchmarkOutcome {
    pub report: AudioBenchmark,
    /// Worst observed steady-state cost, in milliseconds, of converting one
    /// chunk of L16 through the rate bridge at each [`DspQuality::ALL`]
    /// profile.
    pub profile_costs_ms: [f64; 3],
    pub recommended_quality: DspQuality,
}

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

/// The DSP quality profile the measured timings can afford.
///
/// Same safety-factor philosophy as [`recommendation`]: the buffer rule
/// budgets for *three times* the observed worst round trip, so one bad
/// stretch cannot cause an overrun. A profile is therefore recommended only
/// when three times its projected worst-case round trip still fits inside
/// one chunk period — the real-time deadline for one 1024-frame chunk. The
/// projected cost of each profile is *measured*, not assumed: the benchmark
/// times the swappable rate-bridge work at each profile on the same machine
/// and chunk (`bridge_costs_ms`, in [`DspQuality::ALL`] order) and
/// substitutes it into the round trip actually observed at `measured_at`.
/// A guessed cost multiplier would be an unmeasured number presented as
/// fact; measuring keeps the recommendation honest on the machine that will
/// run it. When no profile keeps the safety margin, the cheapest one is the
/// only sensible answer, so [`DspQuality::Economy`] is the fallback.
pub fn recommend_quality(
    worst_case_ms: f64,
    chunk_ms: f64,
    measured_at: DspQuality,
    bridge_costs_ms: &[f64; 3],
) -> DspQuality {
    let cost = |quality: DspQuality| {
        bridge_costs_ms[DspQuality::ALL
            .iter()
            .position(|profile| *profile == quality)
            .expect("quality is one of DspQuality::ALL")]
    };
    let base = (worst_case_ms - cost(measured_at)).max(0.0);
    DspQuality::ALL
        .iter()
        .rev()
        .copied()
        .find(|quality| (base + cost(*quality)) * SAFETY_FACTOR <= chunk_ms)
        .unwrap_or(DspQuality::Economy)
}

/// Worst observed steady-state cost, in milliseconds, of converting one
/// chunk of L16 through the rate bridge at each profile ([`DspQuality::ALL`]
/// order). One warm-up chunk per bridge, then [`COST_PROBES`] timed ones —
/// the same per-chunk work the round-trip timing includes, isolated.
fn profile_costs_ms(input_hz: u32, output_hz: u32, pcm: &[u8]) -> [f64; 3] {
    let mut costs = [0.0f64; 3];
    let mut converted = Vec::new();
    for (index, quality) in DspQuality::ALL.iter().enumerate() {
        let mut bridge = RateBridge::new(input_hz, output_hz, PIPELINE_CHANNELS, *quality);
        bridge.process_l16_into(pcm, &mut converted);
        for _ in 0..COST_PROBES {
            let started = Instant::now();
            bridge.process_l16_into(pcm, &mut converted);
            costs[index] = costs[index].max(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    costs
}

/// Run the isolated check at `quality` — the profile the user currently
/// plays at, so the round-trip timings describe their real configuration.
pub fn run(
    input_hz: u32,
    output_hz: u32,
    gains: [f32; 5],
    iterations: usize,
    quality: DspQuality,
) -> Result<BenchmarkOutcome, String> {
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
        quality,
    );
    let result = (|| {
        let samples: Vec<_> = (0..CHUNK_FRAMES)
            .flat_map(|i| [(i as f32 * 0.05).sin() * 0.3; PIPELINE_CHANNELS])
            .collect();
        let mut bridge = RateBridge::new(input_hz, output_hz, 2, quality);
        let mut converted = Vec::new();
        let quantum = Duration::from_secs_f64(CHUNK_FRAMES as f64 / f64::from(input_hz));
        let started = Instant::now();
        let mut deadline = started;
        let mut timings = Vec::with_capacity(iterations);
        let mut last_chunk = Vec::new();
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
            let chunk = loop {
                match receiver.try_recv() {
                    Ok(chunk) => break chunk.pcm,
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
            bridge.process_l16_into(&chunk, &mut converted);
            std::hint::black_box(&converted);
            let pipeline = dispatched.elapsed().as_secs_f64() * 1000.0;
            if index >= 4 {
                timings.push((pipeline, scheduler));
            }
            last_chunk.clear();
            last_chunk.extend_from_slice(&chunk);
            // Do not compound a temporary missed deadline into every later
            // sample. Each iteration probes a fresh scheduling interval.
            deadline = Instant::now() + quantum.saturating_sub(dispatched.elapsed());
        }
        let worst = timings.iter().map(|(p, s)| p + s).fold(0.0, f64::max);
        let chunk_ms = quantum.as_secs_f64() * 1000.0;
        let (recommended, within_buffer_budget) = recommendation(worst, chunk_ms);
        let bridge_costs_ms = profile_costs_ms(input_hz, output_hz, &last_chunk);
        let recommended_quality = recommend_quality(worst, chunk_ms, quality, &bridge_costs_ms);
        let mut pipeline: Vec<_> = timings.iter().map(|(p, _)| *p).collect();
        let mut scheduler: Vec<_> = timings.iter().map(|(_, s)| *s).collect();
        Ok(BenchmarkOutcome {
            report: AudioBenchmark {
                input_hz,
                output_hz,
                duration_ms: started.elapsed().as_millis() as u64,
                pipeline_p95_ms: p95(&mut pipeline),
                scheduler_p95_ms: p95(&mut scheduler),
                worst_case_ms: worst,
                recommended_buffer_ms: recommended,
                within_buffer_budget,
                recommended_quality,
            },
            profile_costs_ms: bridge_costs_ms,
            recommended_quality,
        })
    })();
    processing.stop();
    result
}
