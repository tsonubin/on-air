use tokio::net::TcpListener;

#[tokio::test]
async fn serve_responds_to_status_over_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        on_air_core::serve(listener).await.unwrap();
    });

    let url = format!("http://{addr}/api/status");
    let response = reqwest::get(url).await.unwrap();
    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    let outputs: serde_json::Value = reqwest::get(format!("http://{addr}/api/outputs"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(outputs["outputs"].is_array());
}

#[tokio::test]
async fn stream_urls_use_the_port_the_core_is_served_on() {
    let state = on_air_core::state::CoreState::new_mock().await;
    let lan_ip = std::net::IpAddr::from([192, 168, 1, 5]);
    assert_eq!(
        state.stream_url(lan_ip, "abc"),
        format!(
            "http://192.168.1.5:{}/stream/abc/audio.wav",
            on_air_core::DEFAULT_PORT
        ),
        "a state that never served falls back to the default port"
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    assert_ne!(addr.port(), on_air_core::DEFAULT_PORT);
    let served = state.clone();
    tokio::spawn(async move {
        on_air_core::serve_with_state(listener, served)
            .await
            .unwrap();
    });
    let response = reqwest::get(format!("http://{addr}/api/status"))
        .await
        .unwrap();
    assert!(response.status().is_success());

    assert_eq!(
        state.stream_url(lan_ip, "abc"),
        format!("http://192.168.1.5:{}/stream/abc/audio.wav", addr.port())
    );
}

#[tokio::test]
async fn a_shutdown_signal_stops_audio_and_releases_the_inhibitor() {
    let (keep_awake, spawner) = on_air_core::keep_awake::testing::recording(true);
    let state = on_air_core::state::CoreState::new_mock()
        .await
        .with_keep_awake(keep_awake);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = state.clone();
    let server = tokio::spawn(async move {
        on_air_core::serve_with_state_until(listener, served, async {
            let _ = stopped.await;
        })
        .await
    });
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{addr}/api/outputs/active"))
        .json(&serde_json::json!({"transport": "sonos", "device_id": "uuid:mock-sonos"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert!(state.active_output().is_some());

    stop.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), server)
        .await
        .expect("serve returns after the signal")
        .unwrap()
        .unwrap();
    assert!(state.active_output().is_none(), "audio stopped on shutdown");
    assert_eq!(
        spawner.events().last(),
        Some(&on_air_core::keep_awake::testing::Event::Released),
        "inhibitor released on shutdown"
    );
}
