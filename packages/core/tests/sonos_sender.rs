use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use on_air_core::sender::sonos::discovery::SonosDevice;
use on_air_core::sender::sonos::SonosSender;
use on_air_core::sender::AudioSender;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>); // soapaction values, in call order

async fn capture_handler(
    State(state): State<Captured>,
    headers: HeaderMap,
    _body: String,
) -> &'static str {
    let action = headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    state.0.lock().unwrap().push(action);
    "OK"
}

async fn start_fake_sonos() -> (std::net::SocketAddr, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/MediaRenderer/AVTransport/Control", post(capture_handler))
        .route(
            "/MediaRenderer/RenderingControl/Control",
            post(capture_handler),
        )
        .with_state(captured.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, captured)
}

#[tokio::test]
async fn start_calls_set_uri_then_play_stop_calls_stop_set_volume_calls_set_volume() {
    let (addr, captured) = start_fake_sonos().await;
    let device = SonosDevice::discovered(
        "uuid:fake",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Fake Speaker",
    );

    let mut sender = SonosSender::new(
        device,
        reqwest::Client::new(),
        "http://192.168.1.10:47990/stream/audio.wav".to_string(),
    );

    sender.start().await.unwrap();
    sender.set_volume(60).await.unwrap();
    sender.stop().await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert!(calls[0].contains("SetAVTransportURI"));
    assert!(calls[1].contains("#Play"));
    assert!(calls[2].contains("SetVolume"));
    assert!(calls[3].contains("#Stop"));
}

#[derive(Clone, Default)]
struct FailingStopProbe(Arc<Mutex<Vec<String>>>);

async fn fail_stop_handler(State(probe): State<FailingStopProbe>, headers: HeaderMap) -> Response {
    let action = headers
        .get("soapaction")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    probe.0.lock().unwrap().push(action.clone());
    if action.contains("#Stop") {
        (StatusCode::INTERNAL_SERVER_ERROR, "stop failed").into_response()
    } else {
        "OK".into_response()
    }
}

#[tokio::test]
async fn failed_stop_preserves_automatic_recovery_for_the_active_sender() {
    use on_air_core::state::{ActiveOutput, CoreState};

    let probe = FailingStopProbe::default();
    let app = Router::new()
        .route(
            "/MediaRenderer/AVTransport/Control",
            post(fail_stop_handler),
        )
        .route(
            "/MediaRenderer/RenderingControl/Control",
            post(fail_stop_handler),
        )
        .with_state(probe.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let device = SonosDevice::discovered(
        "uuid:failed-stop",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Failed Stop",
    );
    let mut state = CoreState::new();
    state.mock = true;
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://127.0.0.1:47990/stream/audio.wav".into(),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone());
    state
        .activate_sender_as(
            Box::new(sender),
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:failed-stop".into(),
                device_name: "Failed Stop".into(),
            }),
        )
        .await
        .unwrap();

    assert!(state.deactivate_sender().await.is_err());
    assert!(state.active_sender.lock().await.is_some());
    let recovered = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let play_count = probe
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|action| action.contains("#Play"))
                .count();
            if play_count >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        recovered.is_ok(),
        "failed Stop left the preserved Sonos sender without recovery"
    );
}
