//! C7 allocation guardrails: steady-state allocation behaviour of the
//! audio path, measured with the counting allocator from
//! [`crate::alloc_counting`].
//!
//! The processing thread runs [`process_until_stopped`] per 1024-frame
//! chunk for as long as the machine plays audio; these tests pin what that
//! loop and its stages allocate once warmed up:
//!
//! - the chunk loop allocates **exactly [`PAYLOAD_ALLOCATIONS_PER_CHUNK`]**
//!   per chunk and nothing else — the L16 payload handed to `audio_tx`
//!   receivers (see [`super::f32_to_le_i16_bytes`] for why that handoff is
//!   the floor with the `Bytes` bus type), and zero per chunk when nothing
//!   is subscribed;
//! - the resampler ([`PcmResampler::process_into`]), the EQ
//!   ([`GraphicEq::process`]) and the meter's level event allocate
//!   **zero** per chunk, at every [`DspQuality`] profile;
//! - the sender-side rate bridge (`crate::dsp::bridge::RateBridge`'s
//!   `*_into` conversions) allocates **zero** per chunk when the caller
//!   reuses its buffer.
//!
//! The per-output delay line of the roadmap (B2) is not in this tree (it
//! lands with the playout-timeline work); when it arrives it must pass the
//! same treatment.
//!
//! Each measurement runs the real code on a dedicated thread (the
//! processing thread's own thread-local counter, so parallel tests cannot
//! pollute it) after a warm-up window that absorbs thread start-up, buffer
//! growth and channel block recycling. What is asserted is the *steady
//! state*, one chunk after another, which is what the thread does all day.
use super::*;
use crate::alloc_counting::thread_allocations;
use crate::dsp::DspQuality;

/// The capture pipeline's own justified channel request (roadmap A6): the
/// guard measures exactly the construction the pipeline drives, plan and
/// all.
fn pipeline_plan() -> crate::dsp::channel::ChannelPlan {
    crate::dsp::channel::SinkConstruction::ProcessingThread
        .auto(crate::dsp::channel::Destination::stereo("allocation guard"))
}

/// Frames per processed chunk: the pipeline's chunk size (the resampler
/// takes its default fixed input size at every profile).
const CHUNK_FRAMES: usize = 1024;

/// The allocation count the processing thread is allowed per chunk in
/// steady state: one — the L16 payload handed to receivers. Everything
/// else on the thread allocates zero (see the module docs).
const PAYLOAD_ALLOCATIONS_PER_CHUNK: u64 = 1;

/// Chunks processed before the measured window (start-up, buffer growth).
const WARMUP_CHUNKS: usize = 32;
/// Chunks in the measured window.
const MEASURED_CHUNKS: usize = 64;

/// The processing thread's allocation count at each chunk boundary,
/// taken in the test-only stage hook (which runs per chunk on exactly the
/// thread being measured).
#[derive(Default)]
struct Marks {
    at: Vec<u64>,
}

/// Runs [`process_until_stopped`] over `WARMUP_CHUNKS + MEASURED_CHUNKS + 1`
/// pre-filled chunks on a dedicated thread and returns the allocation
/// count at every chunk boundary.
fn chunk_allocations(
    input_hz: u32,
    output_hz: u32,
    gains: [f32; 5],
    quality: DspQuality,
    audio_receiver: bool,
    meter_receiver: bool,
) -> Vec<u64> {
    let total = WARMUP_CHUNKS + MEASURED_CHUNKS + 1;
    let (mut producer, consumer) = new_ring_buffer(total * CHUNK_FRAMES);
    let chunk: Vec<f32> = (0..CHUNK_FRAMES)
        .flat_map(|frame| [(frame as f32 * 0.05).sin() * 0.3; PIPELINE_CHANNELS])
        .collect();
    for _ in 0..total {
        let pushed = push_frames(&mut producer, &chunk, PIPELINE_CHANNELS);
        assert_eq!(
            pushed,
            CHUNK_FRAMES * PIPELINE_CHANNELS,
            "ring fits all chunks"
        );
    }

    let (audio_tx, audio_rx) = broadcast::channel(8);
    let (ws_tx, ws_rx) = broadcast::channel(8);
    // The receivers must exist (and outlive the run) for the chunk loop to
    // do its work; they never read, so the channels stay lossy-lagging.
    let _audio_rx = audio_receiver.then_some(audio_rx);
    let _ws_rx = meter_receiver.then_some(ws_rx);
    let outputs = ProcessingOutputs {
        eq_gains_db: Arc::new(Mutex::new(gains)),
        audio_tx,
        ws_tx,
        dsp_quality: quality,
    };

    let marks = Arc::new(Mutex::new(Marks {
        at: Vec::with_capacity(total),
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let stage_marks = marks.clone();
    let stage_stop = stop.clone();
    let stage: ProcessingStage = Box::new(move |_: &mut [f32]| {
        let mut marks = stage_marks.lock();
        marks.at.push(thread_allocations());
        if marks.at.len() >= total {
            stage_stop.store(true, Ordering::Relaxed);
        }
    });

    let thread_stop = stop.clone();
    let worker = std::thread::spawn(move || {
        process_until_stopped(
            consumer,
            ProcessingRates {
                input_hz,
                output_hz,
            },
            outputs,
            Some(stage),
            &thread_stop,
        );
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let done = marks.lock().at.len();
        if done >= total {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "processing stalled after {done} of {total} chunks"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    stop.store(true, Ordering::Relaxed);
    worker.join().expect("processing thread completes");
    let taken = std::mem::take(&mut marks.lock().at);
    taken
}

/// Every boundary in the measured window must show exactly `expected` new
/// allocations on the processing thread.
fn assert_steady(marks: &[u64], expected: u64, what: &str) {
    let window = &marks[WARMUP_CHUNKS..WARMUP_CHUNKS + MEASURED_CHUNKS + 1];
    for (index, delta) in window.windows(2).map(|w| w[1] - w[0]).enumerate() {
        assert_eq!(
            delta, expected,
            "{what}: chunk {index} of the steady-state window allocated {delta} times \
             (expected {expected})"
        );
    }
}

#[test]
fn the_processing_thread_allocates_only_the_payload_per_chunk() {
    for quality in DspQuality::ALL {
        let marks = chunk_allocations(
            44_100,
            48_000,
            [3.0, -2.0, 0.0, 4.5, -1.0],
            quality,
            true,
            true,
        );
        assert_steady(
            &marks,
            PAYLOAD_ALLOCATIONS_PER_CHUNK,
            &format!("chunk loop with a receiver at {quality:?}"),
        );
    }
}

#[test]
fn the_processing_thread_allocates_nothing_per_chunk_without_a_receiver() {
    // Nothing subscribed: the loop skips the DSP and the handoff entirely.
    let marks = chunk_allocations(44_100, 48_000, [0.0; 5], DspQuality::Economy, false, false);
    assert_steady(&marks, 0, "chunk loop without receivers");
    // A meter-only subscriber runs the EQ and the meter but hands off no
    // payload: still zero.
    let marks = chunk_allocations(
        44_100,
        48_000,
        [3.0, -2.0, 0.0, 4.5, -1.0],
        DspQuality::Economy,
        false,
        true,
    );
    assert_steady(&marks, 0, "chunk loop with a meter-only receiver");
}

#[test]
fn the_resampler_allocates_nothing_per_chunk() {
    for quality in DspQuality::ALL {
        let mut resampler =
            crate::dsp::resample::PcmResampler::new(44_100, 48_000, &pipeline_plan(), quality);
        let frames = resampler.input_frames_next();
        assert_eq!(frames, CHUNK_FRAMES, "fixed input chunk at {quality:?}");
        let input = vec![0.3f32; frames * PIPELINE_CHANNELS];
        let mut out = Vec::new();
        for _ in 0..16 {
            resampler.process_into(&input, &mut out);
        }
        let before = thread_allocations();
        for _ in 0..MEASURED_CHUNKS {
            resampler.process_into(&input, &mut out);
        }
        let allocated = thread_allocations() - before;
        assert_eq!(
            allocated, 0,
            "resampler allocated {allocated} times in {MEASURED_CHUNKS} chunks at {quality:?}"
        );
    }
}

#[test]
fn the_eq_allocates_nothing_per_chunk() {
    let mut eq = crate::dsp::eq::GraphicEq::new(44_100.0, PIPELINE_CHANNELS);
    let gains = [3.0, -2.0, 0.0, 4.5, -1.0];
    eq.set_gains_db(gains);
    let mut chunk = vec![0.25f32; CHUNK_FRAMES * PIPELINE_CHANNELS];
    for _ in 0..8 {
        eq.set_gains_db(gains);
        eq.process(&mut chunk);
    }
    let before = thread_allocations();
    for _ in 0..MEASURED_CHUNKS {
        eq.set_gains_db(gains);
        eq.process(&mut chunk);
    }
    let allocated = thread_allocations() - before;
    assert_eq!(
        allocated, 0,
        "EQ allocated {allocated} times in {MEASURED_CHUNKS} chunks"
    );
}

#[test]
fn the_meter_allocates_nothing_per_chunk() {
    // The level event's broadcast handoff: the meter's per-chunk output.
    let event = || WsEvent::LevelMeter {
        left: MeterLevel {
            rms: 0.1,
            peak: 0.2,
        },
        right: MeterLevel {
            rms: 0.1,
            peak: 0.2,
        },
    };
    let (ws_tx, _ws_rx) = broadcast::channel(8);
    for _ in 0..64 {
        let _ = ws_tx.send(event());
    }
    let before = thread_allocations();
    for _ in 0..MEASURED_CHUNKS {
        let _ = ws_tx.send(event());
    }
    let allocated = thread_allocations() - before;
    assert_eq!(
        allocated, 0,
        "meter event allocated {allocated} times in {MEASURED_CHUNKS} chunks"
    );
}

#[test]
fn the_rate_bridge_allocates_nothing_per_chunk() {
    let pcm = {
        let mut pcm = Vec::new();
        crate::dsp::bridge::f32_to_l16_le_into(
            &vec![0.2f32; CHUNK_FRAMES * 2],
            &mut crate::dsp::bridge::TpdfDither::new(1),
            &mut pcm,
        );
        pcm
    };
    for quality in DspQuality::ALL {
        let mut bridge =
            crate::dsp::bridge::RateBridge::new(44_100, 48_000, &pipeline_plan(), quality);
        let mut converted = Vec::new();
        let mut encoded = Vec::new();
        for _ in 0..16 {
            bridge.process_l16_into(&pcm, &mut converted);
            bridge.process_l16_to_l16_into(&pcm, &mut encoded);
        }
        let before = thread_allocations();
        for _ in 0..MEASURED_CHUNKS {
            bridge.process_l16_into(&pcm, &mut converted);
            bridge.process_l16_to_l16_into(&pcm, &mut encoded);
        }
        let allocated = thread_allocations() - before;
        assert_eq!(
            allocated, 0,
            "rate bridge allocated {allocated} times in {MEASURED_CHUNKS} chunks at {quality:?}"
        );
    }
}
