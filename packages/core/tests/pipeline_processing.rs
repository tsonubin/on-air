use bytes::Bytes;
use on_air_core::pipeline::{new_ring_buffer, spawn_processing_task};
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

    let chunk: Bytes = tokio::time::timeout(Duration::from_secs(2), audio_rx.recv())
        .await
        .expect("received a chunk before timing out")
        .expect("channel not closed");

    assert!(!chunk.is_empty());
    assert_eq!(chunk.len() % 2, 0, "L16 PCM is 2 bytes per sample");

    tokio::task::spawn_blocking(move || handle.stop())
        .await
        .unwrap();
}
