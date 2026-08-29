use futures_util::StreamExt;
use on_air_core::state::{ActiveOutput, CoreState};
use tokio::net::TcpListener;

#[tokio::test]
async fn streams_published_pcm_chunks_with_correct_content_type() {
    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
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

#[tokio::test]
async fn stream_is_absent_unless_sonos_is_the_exclusive_output() {
    let state = CoreState::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let response = reqwest::get(format!("http://{addr}/stream/audio.wav"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sonos_radio_is_live_before_play_pulls_the_uri() {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::Router;
    use on_air_core::sender::sonos::discovery::SonosDevice;
    use on_air_core::sender::sonos::SonosSender;
    use on_air_core::sender::AudioSender;
    use on_air_core::state::ActiveOutput;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Captured {
        uri: Arc<Mutex<Option<String>>>,
        play_status: Arc<Mutex<Option<u16>>>,
    }

    async fn soap(State(cap): State<Captured>, headers: HeaderMap, body: String) -> &'static str {
        let action = headers
            .get("soapaction")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if action.contains("SetAVTransportURI") {
            if let Some(rest) = body.split("<CurrentURI>").nth(1) {
                if let Some(uri) = rest.split("</CurrentURI>").next() {
                    if uri.starts_with("http://") {
                        *cap.uri.lock().unwrap() = Some(uri.to_string());
                    }
                }
            }
        }
        if action.contains("#Play") {
            let uri = cap.uri.lock().unwrap().clone();
            let status_slot = cap.play_status.clone();
            tokio::spawn(async move {
                if let Some(uri) = uri {
                    let client = reqwest::Client::builder()
                        .no_proxy()
                        .timeout(std::time::Duration::from_secs(2))
                        .build()
                        .unwrap();
                    if let Ok(resp) = client.get(uri).send().await {
                        *status_slot.lock().unwrap() = Some(resp.status().as_u16());
                    }
                }
            });
        }
        "OK"
    }

    let cap = Captured::default();
    let fake = Router::new()
        .route("/MediaRenderer/AVTransport/Control", post(soap))
        .route("/MediaRenderer/RenderingControl/Control", post(soap))
        .with_state(cap.clone());
    let fake_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake_addr = fake_l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(fake_l, fake).await.unwrap();
    });

    let state = CoreState::new();
    let core_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core_addr = core_l.local_addr().unwrap();
    let app = on_air_core::build_router(state.clone());
    tokio::spawn(async move {
        axum::serve(core_l, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let device = SonosDevice::discovered(
        "uuid:fake",
        format!("http://{fake_addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Fake",
    );
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        format!("http://{core_addr}/stream/audio.wav"),
    );
    state
        .activate_sender_as(
            Box::new(sender) as Box<dyn AudioSender>,
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:fake".into(),
                device_name: "Fake".into(),
            }),
        )
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let status = loop {
        if let Some(code) = *cap.play_status.lock().unwrap() {
            break Some(code);
        }
        if tokio::time::Instant::now() >= deadline {
            break None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(status, Some(200), "speaker GET during Play must not 404");
}
