use futures_util::StreamExt;
use on_air_core::state::CoreState;
use tokio::net::TcpListener;

#[tokio::test]
async fn streams_published_pcm_chunks_with_correct_content_type() {
    let state = CoreState::new();
    let audio_tx = state.audio_tx.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // give the server a moment to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let known_chunk = bytes::Bytes::from_static(&[1, 2, 3, 4]);
    let audio_tx_clone = audio_tx.clone();
    let known_chunk_clone = known_chunk.clone();
    tokio::spawn(async move {
        // keep publishing until a subscriber (the HTTP request below) picks one up
        loop {
            let _ = audio_tx_clone.send(known_chunk_clone.clone());
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });

    let response = reqwest::get(format!("http://{addr}/stream/audio.wav"))
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "audio/wav"
    );

    let mut stream = response.bytes_stream();
    let first_chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("received a chunk before timing out")
        .expect("stream not closed")
        .expect("chunk read ok");
    assert!(first_chunk.starts_with(b"RIFF"), "wav header first: {first_chunk:?}");

    let pcm_chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("received pcm before timing out")
        .expect("stream not closed")
        .expect("chunk read ok");
    assert_eq!(pcm_chunk, known_chunk);
}
