use axum::extract::State;
use axum::http::HeaderMap;
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

async fn capture_handler(State(state): State<Captured>, headers: HeaderMap, _body: String) -> &'static str {
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
        .route("/MediaRenderer/RenderingControl/Control", post(capture_handler))
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
    let device = SonosDevice {
        usn: "uuid:fake".into(),
        location: format!("http://{addr}/xml/device_description.xml"),
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        friendly_name: "Fake Speaker".into(),
    };

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
