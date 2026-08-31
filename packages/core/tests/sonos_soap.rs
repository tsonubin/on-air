use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::Router;
use on_air_core::sender::sonos::soap::SonosControlClient;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(String, String)>>>); // (soapaction, body)

async fn capture_handler(
    State(state): State<Captured>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    let action = headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    state.0.lock().unwrap().push((action, body));
    StatusCode::OK
}

async fn start_fake_upnp_server() -> (std::net::SocketAddr, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/Control", post(capture_handler))
        .with_state(captured.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, captured)
}

#[tokio::test]
async fn set_av_transport_uri_sends_expected_action_and_body() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let control_url = format!("http://{addr}/Control");

    client
        .set_av_transport_uri(&control_url, "http://192.168.1.10:47990/stream/audio.wav")
        .await
        .unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.contains("SetAVTransportURI"));
    assert!(calls[0]
        .1
        .contains("<CurrentURI>http://192.168.1.10:47990/stream/audio.wav</CurrentURI>"));
    assert!(calls[0].1.contains("CurrentURIMetaData"));
    assert!(calls[0].1.contains("audio/wav"));
}

#[tokio::test]
async fn play_and_stop_send_expected_actions() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let control_url = format!("http://{addr}/Control");

    client.play(&control_url).await.unwrap();
    client.stop(&control_url).await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].0.contains("#Play"));
    assert!(calls[1].0.contains("#Stop"));
}

#[tokio::test]
async fn set_volume_sends_desired_volume() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let rendering_url = format!("http://{addr}/Control");

    client.set_volume(&rendering_url, 42).await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.contains("SetVolume"));
    assert!(calls[0].1.contains("<DesiredVolume>42</DesiredVolume>"));
}

#[tokio::test]
async fn non_success_status_is_an_error() {
    let app = Router::new().route(
        "/Control",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "soap fault") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = SonosControlClient::new(reqwest::Client::new());
    let result = client.play(&format!("http://{addr}/Control")).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("500"));
}
