//! Transport switching through real sender implementations.
//!
//! The old rule ("transports are mutually exclusive", one per process) is
//! retired; what remains is per session: switching inside one session stops
//! the previous transport first, and no two sessions may drive the same
//! receiver (hard refusal). Simultaneous multi-transport playback is
//! first-class through sync groups of sessions — see
//! `sonos_and_bluetooth_play_simultaneously_in_one_sync_group` below.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::put;
use axum::Router;
use on_air_core::sender::airplay::AirPlaySender;
use on_air_core::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, BluetoothPlaybackConfig, BluetoothSender,
    MockBluetoothAdapter, RecordingPcmSink,
};
use on_air_core::sender::sonos::discovery::SonosDevice;
use on_air_core::sender::sonos::SonosSender;
use on_air_core::session::OutputSession;
use on_air_core::state::CoreState;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>);

async fn capture(
    State(state): State<Captured>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> StatusCode {
    state.0.lock().unwrap().push(uri.path().to_string());
    StatusCode::OK
}

#[tokio::test]
async fn switching_sonos_airplay_bluetooth_stops_the_previous_sender() {
    let captured = Captured::default();
    let app = Router::new()
        .route("/api/outputs", axum::routing::get(capture))
        .route("/api/outputs/{id}", put(capture))
        .route("/api/player/play", put(capture))
        .route("/api/player/stop", put(capture))
        .route("/api/player/volume", put(capture))
        .route("/api/queue/clear", put(capture))
        .with_state(captured.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let owntone = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let soap_captured = Captured::default();
    let soap_app = Router::new()
        .route(
            "/MediaRenderer/AVTransport/Control",
            axum::routing::post(capture),
        )
        .route(
            "/MediaRenderer/RenderingControl/Control",
            axum::routing::post(capture),
        )
        .with_state(soap_captured.clone());
    let soap_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let soap_addr = soap_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(soap_listener, soap_app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

    let adapter = Arc::new(MockBluetoothAdapter::with_devices(vec![BluetoothDevice {
        id: "bt-1".into(),
        name: "BT".into(),
        paired: true,
        connected: false,
        audio_endpoint: Some("bt-1".into()),
    }]));

    let state = CoreState::new();
    let sonos = SonosDevice::discovered(
        "uuid:s",
        format!("http://{soap_addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Sonos",
    );

    state
        .activate_sender(Box::new(SonosSender::new(
            sonos,
            reqwest::Client::builder().no_proxy().build().unwrap(),
            "http://127.0.0.1:47990/stream/audio.wav".into(),
        )))
        .await
        .unwrap();
    state
        .activate_sender(Box::new(AirPlaySender::new(
            "AirPlay",
            "ap-1",
            format!("http://{owntone}"),
        )))
        .await
        .unwrap();
    let sink = std::sync::Arc::new(RecordingPcmSink::default());
    state
        .activate_sender(Box::new(BluetoothSender::new(
            adapter.list()[0].clone(),
            adapter.clone(),
            state.audio_tx.clone(),
            Arc::new(sink),
            BluetoothPlaybackConfig {
                pipeline_hz: 44_100,
                output_hz: 44_100,
                volume: 100,
                sync: None,
            },
        )))
        .await
        .unwrap();

    let bt = adapter.list();
    assert!(bt[0].connected);
    let paths = captured.0.lock().unwrap().clone();
    assert!(
        paths.iter().any(|p| p.contains("/api/player/stop")),
        "airplay stop must run before bluetooth starts, got {paths:?}"
    );
}

#[tokio::test]
async fn mock_http_golden_path_picks_input_activates_and_sets_eq_volume() {
    let state = CoreState::new_mock().await;
    let app = on_air_core::build_router(state);

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let inputs = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/inputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(inputs.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["inputs"][0], "Mock Monitor");

    let activate_in = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inputs/active")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Mock Monitor"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate_in.status(), axum::http::StatusCode::NO_CONTENT);

    let outputs = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/outputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(outputs.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let sonos = json["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["transport"] == "sonos")
        .unwrap();

    let activate_out = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"transport":"sonos","device_id":"{}"}}"#,
                    sonos["id"].as_str().unwrap()
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate_out.status(), axum::http::StatusCode::NO_CONTENT);

    let volume = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active/volume")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"volume":20}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(volume.status(), axum::http::StatusCode::NO_CONTENT);

    let eq = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/eq")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"gains_db":[3.0,0.0,0.0,0.0,-3.0]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(eq.status(), axum::http::StatusCode::NO_CONTENT);
}

/// Two real transports, two sessions, one sync group: Sonos and Bluetooth
/// play at the same time. The old "transports are mutually exclusive" rule
/// is retired at the process level and lives on as one sender per session
/// (`switching_sonos_airplay_bluetooth_stops_the_previous_sender`, above,
/// still pins that half).
#[tokio::test]
async fn sonos_and_bluetooth_play_simultaneously_in_one_sync_group() {
    let soap_captured = Captured::default();
    let soap_app = Router::new()
        .route(
            "/MediaRenderer/AVTransport/Control",
            axum::routing::post(capture),
        )
        .route(
            "/MediaRenderer/RenderingControl/Control",
            axum::routing::post(capture),
        )
        .with_state(soap_captured.clone());
    let soap_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let soap_addr = soap_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(soap_listener, soap_app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

    let adapter = Arc::new(MockBluetoothAdapter::with_devices(vec![BluetoothDevice {
        id: "bt-1".into(),
        name: "BT".into(),
        paired: true,
        connected: false,
        audio_endpoint: Some("bt-1".into()),
    }]));
    let sonos = SonosDevice::discovered(
        "uuid:s",
        format!("http://{soap_addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Sonos",
    );

    let state = CoreState::new();
    let group = state.sync.create_group();
    let sonos_identity = on_air_core::state::ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:s".into(),
        device_name: "Sonos".into(),
    };
    let bt_identity = on_air_core::state::ActiveOutput {
        transport: "bluetooth".into(),
        device_id: "bt-1".into(),
        device_name: "BT".into(),
    };
    let sonos_member = state.sync.join(&group, &sonos_identity);
    let bt_member = state.sync.join(&group, &bt_identity);
    let plans = group.schedule_start(std::time::Instant::now());
    let sonos_plan = plans
        .iter()
        .find(|(member, _)| *member == sonos_member)
        .unwrap()
        .1;
    let bt_plan = plans
        .iter()
        .find(|(member, _)| *member == bt_member)
        .unwrap()
        .1;
    assert_eq!(
        sonos_plan.epoch, bt_plan.epoch,
        "the group hands both members one presentation epoch"
    );

    let (ws_tx, _) = tokio::sync::broadcast::channel(8);
    let session_a = OutputSession::new(ws_tx.clone(), Arc::new(parking_lot::Mutex::new(44_100)));
    let session_b = OutputSession::new(ws_tx, Arc::new(parking_lot::Mutex::new(44_100)));
    session_a.set_manage_local_sink(false);
    session_b.set_manage_local_sink(false);

    let sink = std::sync::Arc::new(RecordingPcmSink::default());
    let sonos_start = session_a.activate_scheduled(
        Box::new(SonosSender::new(
            sonos,
            reqwest::Client::builder().no_proxy().build().unwrap(),
            "http://127.0.0.1:47990/stream/audio.wav".into(),
        )),
        Some(sonos_identity),
        None,
        sonos_plan,
    );
    let bt_start = session_b.activate_scheduled(
        Box::new(BluetoothSender::new(
            adapter.list()[0].clone(),
            adapter.clone(),
            state.audio_tx.clone(),
            Arc::new(sink),
            BluetoothPlaybackConfig {
                pipeline_hz: 44_100,
                output_hz: 44_100,
                volume: 100,
                sync: state.sync.feed_for(&bt_identity, 44_100),
            },
        )),
        Some(bt_identity),
        None,
        bt_plan,
    );
    let (sonos_result, bt_result) = tokio::join!(sonos_start, bt_start);
    sonos_result.unwrap();
    bt_result.unwrap();

    // Both live at once: the retired exclusivity rule would have stopped one.
    assert!(adapter.list()[0].connected, "Bluetooth is playing");
    assert!(
        soap_captured
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.contains("AVTransport")),
        "Sonos is playing"
    );
    assert!(session_a.sender_name().await.is_some());
    assert!(session_b.sender_name().await.is_some());

    session_a.deactivate().await.unwrap();
    session_b.deactivate().await.unwrap();
}
