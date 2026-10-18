use on_air_core::dsp::bridge::l16_le_to_f32;
use on_air_core::dsp::DspQuality;
use on_air_core::pipeline::{new_ring_buffer, push_frames, spawn_processing_task};
use parking_lot::Mutex;
use ringbuf::traits::Producer;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[tokio::test]
async fn processes_synthetic_frames_into_broadcast_pcm() {
    let (mut producer, consumer) = new_ring_buffer(1024 * 8);
    let (audio_tx, mut audio_rx) = broadcast::channel(8);
    let (ws_tx, _) = broadcast::channel::<on_air_core::api::ws::WsEvent>(8);
    let eq_gains_db = Arc::new(Mutex::new([0.0; 5]));

    let handle = spawn_processing_task(
        consumer,
        44100,
        44100,
        eq_gains_db,
        audio_tx,
        ws_tx,
        DspQuality::Economy,
    );

    // push more than one resampler chunk's worth of a known sine wave
    let samples: Vec<f32> = (0..1024 * 2)
        .map(|i| (i as f32 * 0.05).sin() * 0.4)
        .collect();
    let mut pushed = 0;
    while pushed < samples.len() {
        pushed += producer.push_slice(&samples[pushed..]);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let chunk = tokio::time::timeout(Duration::from_secs(2), audio_rx.recv())
        .await
        .expect("received a chunk before timing out")
        .expect("channel not closed")
        .pcm;

    assert!(!chunk.is_empty());
    assert_eq!(
        chunk.len() % 4,
        0,
        "a bus chunk holds whole stereo L16 frames"
    );

    tokio::task::spawn_blocking(move || handle.stop())
        .await
        .unwrap();
}

#[test]
fn push_frames_never_splits_a_frame() {
    use ringbuf::traits::{Consumer, Split};
    let (mut producer, mut consumer) = ringbuf::HeapRb::<f32>::new(5).split();
    assert_eq!(
        push_frames(&mut producer, &[0.1, -0.1, 0.2, -0.2, 0.3, -0.3], 2),
        4
    );
    let mut popped = [0.0; 4];
    assert_eq!(consumer.pop_slice(&mut popped), 4);
    assert_eq!(popped, [0.1, -0.1, 0.2, -0.2]);
    assert_eq!(push_frames(&mut producer, &[0.3, -0.3], 2), 2);
}

/// Runs interleaved stereo `frames` through a processing thread and returns
/// at least 2048 output frames, decoded and still interleaved.
async fn through_the_pipeline(
    input_hz: u32,
    output_hz: u32,
    gains_db: [f32; 5],
    frames: &[f32],
) -> Vec<f32> {
    let (mut producer, consumer) = new_ring_buffer(1024 * 8);
    let (audio_tx, mut audio_rx) = broadcast::channel(64);
    let (ws_tx, _) = broadcast::channel::<on_air_core::api::ws::WsEvent>(8);
    let gains = Arc::new(Mutex::new(gains_db));
    let handle = spawn_processing_task(
        consumer,
        input_hz,
        output_hz,
        gains,
        audio_tx,
        ws_tx,
        DspQuality::Economy,
    );

    let mut pushed = 0;
    while pushed < frames.len() {
        pushed += push_frames(&mut producer, &frames[pushed..], 2);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let mut pcm = Vec::new();
    while pcm.len() < 2048 * 4 {
        let chunk = tokio::time::timeout(Duration::from_secs(2), audio_rx.recv())
            .await
            .unwrap()
            .unwrap()
            .pcm;
        assert_eq!(
            chunk.len() % 4,
            0,
            "a bus chunk holds whole stereo L16 frames"
        );
        pcm.extend_from_slice(&chunk);
    }
    tokio::task::spawn_blocking(move || handle.stop())
        .await
        .unwrap();
    l16_le_to_f32(&pcm)
}

#[tokio::test]
async fn stereo_frames_keep_their_sides_through_the_eq() {
    let tone = |n: usize| (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 44_100.0).sin() * 0.25;
    let frames: Vec<f32> = (0..4096).flat_map(|n| [tone(n), 0.0]).collect();
    let out = through_the_pipeline(44_100, 44_100, [0.0, 0.0, 12.0, 0.0, 0.0], &frames).await;
    assert!(out
        .as_chunks::<2>()
        .0
        .iter()
        .any(|[left, _]| left.abs() > 0.1));
    assert!(
        out.as_chunks::<2>()
            .0
            .iter()
            .all(|[_, right]| *right == 0.0),
        "the boosted left tone leaked into the right channel"
    );
}

#[tokio::test]
async fn resampled_stereo_frames_keep_their_sides() {
    let frames = [0.5f32, -0.5].repeat(4096);
    let out = through_the_pipeline(48_000, 44_100, [0.0; 5], &frames).await;
    for frame in out.as_chunks::<2>().0.iter().skip(64) {
        assert!(
            (frame[0] - 0.5).abs() < 0.02 && (frame[1] + 0.5).abs() < 0.02,
            "{frame:?}"
        );
    }
}

/// The next `LevelMeter` event on the channel, or panic on timeout.
async fn next_meter(
    ws_rx: &mut tokio::sync::broadcast::Receiver<on_air_core::api::ws::WsEvent>,
) -> (on_air_api::MeterLevel, on_air_api::MeterLevel) {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), ws_rx.recv())
            .await
            .expect("a meter event before timing out")
            .expect("the processing thread is still publishing");
        if let on_air_core::api::ws::WsEvent::LevelMeter { left, right } = event {
            return (left, right);
        }
    }
}

/// A2 invariant: the meter's two channels are independent. A tone on one
/// side must not appear on the other, and a mono (duplicated) capture must
/// read the same on both. This runs the real processing thread, not the
/// meter maths in isolation.
#[tokio::test]
async fn the_meter_keeps_left_and_right_independent() {
    let (mut producer, consumer) = new_ring_buffer(1024 * 8);
    let (audio_tx, _audio_rx) = broadcast::channel(8);
    let (ws_tx, mut ws_rx) = broadcast::channel::<on_air_core::api::ws::WsEvent>(64);
    let handle = spawn_processing_task(
        consumer,
        44_100,
        44_100,
        Arc::new(Mutex::new([0.0; 5])),
        audio_tx,
        ws_tx,
        DspQuality::Economy,
    );
    let tone = |n: usize| (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 44_100.0).sin() * 0.5;
    let push = |producer: &mut _, frames: &[f32]| {
        let mut pushed = 0;
        while pushed < frames.len() {
            pushed += push_frames(producer, &frames[pushed..], 2);
        }
    };

    // One chunk per phase, one meter event per chunk, consumed in order:
    // each assertion therefore reads exactly its own phase.
    // Left only.
    let frames: Vec<f32> = (0..1024).flat_map(|n| [tone(n), 0.0]).collect();
    push(&mut producer, &frames);
    let (left, right) = next_meter(&mut ws_rx).await;
    assert!(
        left.peak > 0.3 && left.rms > 0.2,
        "the left tone is visible on the left meter: {left:?}"
    );
    assert_eq!(
        (right.rms, right.peak),
        (0.0, 0.0),
        "the left tone leaked into the right meter"
    );

    // Right only.
    let frames: Vec<f32> = (0..1024).flat_map(|n| [0.0, tone(n)]).collect();
    push(&mut producer, &frames);
    let (left, right) = next_meter(&mut ws_rx).await;
    assert!(
        right.peak > 0.3 && right.rms > 0.2,
        "the right tone is visible on the right meter: {right:?}"
    );
    assert_eq!(
        (left.rms, left.peak),
        (0.0, 0.0),
        "the right tone leaked into the left meter"
    );

    // A mono capture duplicates into both sides: identical readings.
    let frames: Vec<f32> = (0..1024).flat_map(|n| [tone(n), tone(n)]).collect();
    push(&mut producer, &frames);
    let (left, right) = next_meter(&mut ws_rx).await;
    assert_eq!(
        left, right,
        "a duplicated mono capture reads the same twice"
    );

    tokio::task::spawn_blocking(move || handle.stop())
        .await
        .unwrap();
}
