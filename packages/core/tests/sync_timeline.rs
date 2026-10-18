//! The sync foundation's real invariants: the shared playback timeline, the
//! per-output delay line, drift steering, and one presentation epoch for a
//! sync group. These pin behavior, not wiring.
//!
//! No acoustic or multi-speaker verification is possible on this host (no
//! hardware): everything here is the arithmetic and scheduling the sync
//! machinery guarantees before a calibrated speaker ever hears it.

use bytes::Bytes;
use on_air_core::dsp::channel::{Destination, SinkConstruction};
use on_air_core::dsp::resample::SteerableResampler;
use on_air_core::dsp::DspQuality;
use on_air_core::sender::{AudioSender, SenderError, StartPlan};
use on_air_core::session::OutputSession;
use on_air_core::sync::{DriftController, OutputKey, SyncController};
use on_air_core::timeline::{PcmChunk, PcmDelayLine, FRAME_BYTES};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// 1. Frame indices are contiguous and ordered on the real processing task.
#[tokio::test]
async fn frame_indices_are_contiguous_and_ordered() {
    use on_air_core::pipeline::{new_ring_buffer, push_frames, spawn_processing_task};

    const CHUNK_FRAMES: usize = 1024;
    const CHUNKS: u64 = 48;
    let (mut producer, consumer) = new_ring_buffer(CHUNK_FRAMES * 128);
    let (audio_tx, mut audio_rx) = broadcast::channel::<PcmChunk>(256);
    let (ws_tx, _) = broadcast::channel(8);
    let handle = spawn_processing_task(
        consumer,
        44_100,
        44_100,
        Arc::new(Mutex::new([0.0; 5])),
        audio_tx,
        ws_tx,
        on_air_core::dsp::DspQuality::Economy,
    );

    let frame = [0.1f32, -0.1f32];
    let mut chunk = Vec::with_capacity(CHUNK_FRAMES * 2);
    for _ in 0..CHUNK_FRAMES {
        chunk.extend_from_slice(&frame);
    }
    for _ in 0..CHUNKS {
        assert_eq!(
            push_frames(&mut producer, &chunk, 2),
            chunk.len(),
            "the capture ring held every frame"
        );
    }

    let mut cursor = 0u64;
    let mut last_capture = Instant::now() - Duration::from_secs(1);
    for _ in 0..CHUNKS {
        let received = tokio::time::timeout(Duration::from_secs(5), audio_rx.recv())
            .await
            .expect("a chunk arrived before the timeout")
            .expect("the processing task published");
        assert_eq!(
            received.first_frame, cursor,
            "chunks are ordered and contiguous: no gap, no repeat"
        );
        assert!(
            received.captured_at >= last_capture,
            "capture timestamps never move backwards"
        );
        assert_eq!(received.frames(), CHUNK_FRAMES as u64);
        assert_eq!(received.end_frame(), cursor + CHUNK_FRAMES as u64);
        last_capture = received.captured_at;
        cursor = received.end_frame();
    }
    handle.stop();
}

/// Helper: a stereo chunk of L16 frames carrying their frame index in the
/// left sample, so the delay line's output can be checked sample-exact.
fn tagged_chunk(frames: &[u16]) -> Bytes {
    let mut bytes = Vec::with_capacity(frames.len() * FRAME_BYTES);
    for sample in frames {
        bytes.extend_from_slice(&sample.to_le_bytes());
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Bytes::from(bytes)
}

/// The left sample of each frame across the delivered segments.
fn tagged_samples(parts: Vec<Bytes>) -> Vec<u16> {
    let bytes: Vec<u8> = parts.iter().flat_map(|part| part.iter().copied()).collect();
    bytes
        .as_chunks::<FRAME_BYTES>()
        .0
        .iter()
        .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
        .collect()
}

/// 2. The delay line actually delays by the requested sample count.
#[test]
fn the_delay_line_delays_by_the_requested_sample_count() {
    const DELAY: u64 = 5;
    let mut line = PcmDelayLine::new(DELAY, 2);
    assert_eq!(line.delay_frames(), DELAY);

    let mut delivered: Vec<u16> = Vec::new();
    let mut pushed = 0usize;
    let batches: [&[u16]; 3] = [&[1, 2, 3], &[4, 5, 6, 7], &[8, 9]];
    for batch in batches {
        let push = line.push(tagged_chunk(batch));
        // Output rate equals input rate exactly: a live feed never stalls.
        let frames: usize = push.iter().map(|part| part.len()).sum::<usize>() / FRAME_BYTES;
        assert_eq!(frames, batch.len());
        delivered.extend(tagged_samples(push));
        pushed += batch.len();
    }
    delivered.extend(tagged_samples(line.flush()));

    // Every input frame appears exactly once, `DELAY` frames late, behind
    // `DELAY` frames of silence.
    assert_eq!(delivered.len(), pushed + DELAY as usize);
    assert_eq!(delivered[..DELAY as usize], vec![0; DELAY as usize]);
    assert_eq!(delivered[DELAY as usize..], (1..=9).collect::<Vec<u16>>());
    assert_eq!(line.pending_frames(), 0, "flush emptied the line");
}

/// 3. Drift steering moves the ratio in the right direction and converges.
#[test]
fn drift_steering_moves_the_ratio_in_the_right_direction_and_converges() {
    // A consumer whose clock runs 500 ppm fast must be fed a stretched
    // stream: the ratio climbs above nominal and settles near 1.0005.
    let nominal_hz = 44_100u32;
    let base = Instant::now();
    let mut fast = DriftController::new(nominal_hz);
    let mut frames = 0u64;
    fast.observe(0, base);
    for window in 0..40u64 {
        frames += (f64::from(nominal_hz) * 1.0005).round() as u64;
        fast.observe(frames, base + Duration::from_secs(window + 1));
    }
    let fast_ppm = fast.correction_ppm();
    assert!(
        fast_ppm > 400.0 && fast_ppm < 600.0,
        "fast consumer steers the ratio up and converges near 500 ppm, got {fast_ppm}"
    );

    let mut slow = DriftController::new(nominal_hz);
    let mut frames = 0u64;
    slow.observe(0, base);
    for window in 0..40u64 {
        frames += (f64::from(nominal_hz) * 0.9997).round() as u64;
        slow.observe(frames, base + Duration::from_secs(window + 1));
    }
    let slow_ppm = slow.correction_ppm();
    assert!(
        slow_ppm < -200.0 && slow_ppm > -400.0,
        "slow consumer steers the ratio down and converges near -300 ppm, got {slow_ppm}"
    );

    // The correction lands on a real variable-ratio resampler and changes
    // what it produces.
    let mut steered = SteerableResampler::new(
        nominal_hz,
        nominal_hz,
        &SinkConstruction::ProcessingThread.auto(Destination::stereo("sync timeline test")),
        DspQuality::Balanced,
    );
    let nominal_ratio = steered.ratio();
    steered.set_rate_correction_ppm(fast_ppm);
    let fast_ratio = steered.ratio();
    assert!(
        fast_ratio > nominal_ratio,
        "a fast consumer raises the resample ratio: {nominal_ratio} -> {fast_ratio}"
    );
    steered.set_rate_correction_ppm(slow_ppm);
    assert!(
        steered.ratio() < nominal_ratio,
        "a slow consumer lowers the resample ratio"
    );

    // Frames out track the ratio: more frames per input for the fast one.
    let input = vec![0.1f32; 1024 * 2];
    steered.set_rate_correction_ppm(fast_ppm);
    let mut out = Vec::new();
    let mut fed = 0usize;
    for _ in 0..32 {
        let produced = steered.process(&input);
        fed += input.len() / 2;
        out.extend_from_slice(&produced);
    }
    let produced_frames = out.len() / 2;
    let expected = fed as f64 * fast_ratio;
    assert!(
        (produced_frames as f64 - expected).abs() / expected < 0.01,
        "produced {produced_frames} frames, expected about {expected}"
    );
}

/// Recording sender: notes the plan it was scheduled with and the instant
/// its transport actually began.
type StartEntry = (&'static str, StartPlan, Instant);

#[derive(Clone)]
struct StartLog(Arc<tokio::sync::Mutex<Vec<StartEntry>>>);

struct EpochSender {
    name: &'static str,
    log: StartLog,
}

#[async_trait::async_trait]
impl AudioSender for EpochSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        unreachable!("this sender is always started on a schedule")
    }
    async fn start_at(&mut self, plan: StartPlan) -> Result<(), SenderError> {
        plan.wait().await;
        self.log
            .0
            .lock()
            .await
            .push((self.name, plan, Instant::now()));
        Ok(())
    }
    async fn stop(&mut self) -> Result<(), SenderError> {
        Ok(())
    }
    async fn set_volume(&mut self, _volume: u8) -> Result<(), SenderError> {
        Ok(())
    }
    fn name(&self) -> &str {
        self.name
    }
    fn transport(&self) -> &'static str {
        "null"
    }
}

fn output(transport: &str, device_id: &str, name: &str) -> on_air_api::ActiveOutput {
    on_air_api::ActiveOutput {
        transport: transport.into(),
        device_id: device_id.into(),
        device_name: name.into(),
    }
}

/// 4. Two outputs in a sync group start at the same epoch.
#[tokio::test]
async fn two_outputs_in_a_sync_group_start_at_the_same_epoch() {
    let sync = Arc::new(SyncController::new());
    let group = sync.create_group("Downstairs");
    let kitchen = output("sonos", "uuid:kitchen", "Kitchen");
    let patio = output("bluetooth", "bt-1", "Patio");
    let kitchen_member = sync.join(&group, &kitchen).unwrap();
    let patio_member = sync.join(&group, &patio).unwrap();

    let plans = group.schedule_start(Instant::now() + Duration::from_millis(150));
    assert_eq!(plans.len(), 2, "both members are scheduled");
    let kitchen_plan = plans
        .iter()
        .find(|(member, _)| *member == kitchen_member)
        .expect("kitchen has a plan")
        .1;
    let patio_plan = plans
        .iter()
        .find(|(member, _)| *member == patio_member)
        .expect("patio has a plan")
        .1;

    // One shared presentation epoch, not two fire-now starts.
    assert_eq!(
        kitchen_plan.epoch, patio_plan.epoch,
        "group members share one presentation epoch"
    );

    // The fast transport (Bluetooth, 150 ms) waits for the slow one (Sonos,
    // unmeasured 1000 ms placeholder): its feed holds 850 ms more.
    assert_eq!(kitchen_plan.baseline_latency, Duration::from_millis(1_000));
    assert_eq!(kitchen_plan.delay, Duration::ZERO);
    assert_eq!(patio_plan.delay, Duration::from_millis(850));
    assert_eq!(
        patio_plan.delay_frames(48_000),
        on_air_core::sync::delay_frames(Duration::from_millis(850), 48_000)
    );

    // Both sessions start their senders on that one epoch.
    let log = StartLog(Arc::new(tokio::sync::Mutex::new(Vec::new())));
    let (ws_tx, _) = broadcast::channel(16);
    let session_a = OutputSession::new(ws_tx.clone(), Arc::new(Mutex::new(44_100)));
    let session_b = OutputSession::new(ws_tx, Arc::new(Mutex::new(44_100)));
    session_a.set_manage_local_sink(false);
    session_b.set_manage_local_sink(false);

    let epoch = kitchen_plan.epoch;
    let a = {
        let (session, log, plan) = (session_a.clone(), log.clone(), kitchen_plan);
        tokio::spawn(async move {
            session
                .activate_scheduled(
                    Box::new(EpochSender { name: "A", log }),
                    Some(kitchen.clone()),
                    None,
                    plan,
                )
                .await
        })
    };
    let b = {
        let (session, log, plan) = (session_b.clone(), log.clone(), patio_plan);
        tokio::spawn(async move {
            session
                .activate_scheduled(
                    Box::new(EpochSender { name: "B", log }),
                    Some(patio.clone()),
                    None,
                    plan,
                )
                .await
        })
    };
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();

    let entries = log.0.lock().await;
    assert_eq!(entries.len(), 2, "both members started");
    for (name, plan, started_at) in entries.iter() {
        assert_eq!(plan.epoch, epoch, "{name} saw the shared epoch");
        assert!(
            *started_at + Duration::from_millis(5) >= epoch.start_at,
            "{name} began at the epoch, not fire-now (started {started_at:?}, epoch {:?})",
            epoch.start_at
        );
    }
    session_a.deactivate().await.unwrap();
    session_b.deactivate().await.unwrap();
}

/// The delay line a sync group assigns one member is derived from the
/// baseline minus that member's latency, at the member's own feed rate.
#[test]
fn a_groups_delay_lines_line_every_member_up_at_the_baseline() {
    let sync = Arc::new(SyncController::new());
    let group = sync.create_group("Downstairs");
    let sonos = OutputKey::new("sonos", "uuid:1");
    let bluetooth = OutputKey::new("bluetooth", "bt-1");
    sync.join(&group, &output("sonos", "uuid:1", "Kitchen"))
        .unwrap();
    sync.join(&group, &output("bluetooth", "bt-1", "Patio"))
        .unwrap();
    assert_eq!(group.delay_frames(&sonos, 48_000), Some(0));
    assert_eq!(
        group.delay_frames(&bluetooth, 48_000),
        Some((0.850_f64 * 48_000.0).round() as u64),
        "the 850 ms gap at 48 kHz"
    );
    // A user trim on the fast transport shrinks its delay line.
    group.set_latency(
        group.member_of(&bluetooth).unwrap(),
        on_air_core::sync::effective_latency("bluetooth", None, 700),
    );
    assert_eq!(
        group.delay_frames(&bluetooth, 48_000),
        Some((0.150_f64 * 48_000.0).round() as u64)
    );

    // The same alignment through the feed handle a real output holds: the
    // fast member's feed carries exactly the group's delay line, sized at
    // the feed's own rate, so its PCM is held back to the group baseline.
    let feed = sync
        .feed_for(&output("bluetooth", "bt-1", "Patio"), 48_000)
        .expect("a member's feed is aligned to the group");
    assert_eq!(
        feed.delay_frames(),
        (0.150_f64 * 48_000.0).round() as u64,
        "the feed's delay line is the member's hold at its own rate"
    );
    // And a free-running output gets no feed at all: grouping is more
    // constrained, never less.
    assert!(
        sync.feed_for(&output("sonos", "uuid:free", "Free"), 48_000)
            .is_none(),
        "free-running outputs stay free of the group"
    );
}
