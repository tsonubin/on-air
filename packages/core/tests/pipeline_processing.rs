use on_air_core::dsp::bridge::l16_le_to_f32;
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

    let handle = spawn_processing_task(consumer, 44100, 44100, eq_gains_db, audio_tx, ws_tx);

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
    let handle = spawn_processing_task(consumer, input_hz, output_hz, gains, audio_tx, ws_tx);

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
