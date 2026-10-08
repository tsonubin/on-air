use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::Json;
use axum::Router;
use on_air_core::sender::airplay::fetch_owntone_outputs;
use on_air_core::sender::AudioSender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

static NETWORK_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone)]
struct OwnToneCapture {
    events: Arc<Mutex<Vec<String>>>,
    queue_uri: Arc<Mutex<Option<String>>>,
    queue_status: Arc<AtomicU16>,
    play_status: Arc<AtomicU16>,
    stop_status: Arc<AtomicU16>,
}

impl Default for OwnToneCapture {
    fn default() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            queue_uri: Arc::new(Mutex::new(None)),
            queue_status: Arc::new(AtomicU16::new(StatusCode::NO_CONTENT.as_u16())),
            play_status: Arc::new(AtomicU16::new(StatusCode::NO_CONTENT.as_u16())),
            stop_status: Arc::new(AtomicU16::new(StatusCode::NO_CONTENT.as_u16())),
        }
    }
}

impl OwnToneCapture {
    fn record(&self, event: impl Into<String>) {
        self.events.lock().unwrap().push(event.into());
    }
}

async fn catalog() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "outputs": [{"id": "speaker/id", "name": "HomePod"}]
    }))
}

async fn select_output(
    State(state): State<OwnToneCapture>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> StatusCode {
    state.record(format!("select:{id}:{}", body["selected"]));
    StatusCode::NO_CONTENT
}

async fn add_queue_item(
    State(state): State<OwnToneCapture>,
    Query(query): Query<HashMap<String, String>>,
) -> StatusCode {
    *state.queue_uri.lock().unwrap() = query.get("uris").cloned();
    state.record("queue:add");
    StatusCode::from_u16(state.queue_status.load(Ordering::Relaxed)).unwrap()
}

async fn player_action(
    State(state): State<OwnToneCapture>,
    OriginalUri(uri): OriginalUri,
) -> StatusCode {
    state.record(uri.path().to_string());
    match uri.path() {
        "/api/player/play" => {
            StatusCode::from_u16(state.play_status.load(Ordering::Relaxed)).unwrap()
        }
        "/api/player/stop" => {
            StatusCode::from_u16(state.stop_status.load(Ordering::Relaxed)).unwrap()
        }
        _ => StatusCode::NO_CONTENT,
    }
}

async fn spawn_owntone(state: OwnToneCapture) -> std::net::SocketAddr {
    let app = Router::new()
        .route("/api/outputs", get(catalog))
        .route("/api/outputs/:id", put(select_output))
        .route("/api/queue/items/add", post(add_queue_item))
        .route("/api/player/play", put(player_action))
        .route("/api/player/stop", put(player_action))
        .route("/api/player/volume", put(player_action))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn fetch_owntone_outputs_maps_sidecar_json_into_catalog() {
    let _network = NETWORK_TEST_LOCK.lock().await;
    let app = Router::new().route(
        "/api/outputs",
        get(|| async {
            Json(serde_json::json!({
                "outputs": [
                    {"id": "1", "name": "HomePod"},
                    {"id": "2", "name": "Apple TV"}
                ]
            }))
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

    let devices = fetch_owntone_outputs(&format!("http://{addr}"))
        .await
        .unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].name, "HomePod");
    assert_eq!(devices[1].id, "2");
}

#[tokio::test]
async fn start_without_owntone_or_receiver_host_explains_the_sidecar() {
    let mut sender =
        on_air_core::sender::airplay::AirPlaySender::new("HomePod", "1", "http://127.0.0.1:1");
    let err = sender.start().await.unwrap_err();
    assert!(
        err.to_string().contains("OwnTone") || err.to_string().contains("pyatv"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn owntone_start_and_stop_are_transactional_and_preserve_the_stream_url() {
    let _network = NETWORK_TEST_LOCK.lock().await;
    let capture = OwnToneCapture::default();
    let addr = spawn_owntone(capture.clone()).await;
    let stream_url = "http://192.168.1.5:47990/stream/audio.wav?token=a&room=two words";
    let mut sender = on_air_core::sender::airplay::AirPlaySender::new(
        "HomePod",
        "speaker/id",
        format!("http://{addr}"),
    )
    .with_radio(stream_url, "192.168.1.10");

    sender.start().await.unwrap();
    sender.set_volume(31).await.unwrap();
    sender.stop().await.unwrap();

    assert_eq!(
        capture.queue_uri.lock().unwrap().as_deref(),
        Some(stream_url)
    );
    assert_eq!(
        *capture.events.lock().unwrap(),
        vec![
            "select:speaker/id:true",
            "queue:add",
            "/api/player/play",
            "/api/player/volume",
            "/api/player/stop",
            "select:speaker/id:false",
        ]
    );
}

#[tokio::test]
async fn failed_queue_add_deselects_output_and_does_not_start_player() {
    let _network = NETWORK_TEST_LOCK.lock().await;
    let capture = OwnToneCapture::default();
    capture
        .queue_status
        .store(StatusCode::BAD_GATEWAY.as_u16(), Ordering::Relaxed);
    let addr = spawn_owntone(capture.clone()).await;
    let mut sender = on_air_core::sender::airplay::AirPlaySender::new(
        "HomePod",
        "speaker/id",
        format!("http://{addr}"),
    )
    .with_radio("http://127.0.0.1:47990/stream/audio.wav", "127.0.0.1");

    let error = sender.start().await.unwrap_err();
    assert!(error.to_string().contains("queue add failed"), "{error}");
    sender.stop().await.unwrap();

    assert_eq!(
        *capture.events.lock().unwrap(),
        vec![
            "select:speaker/id:true",
            "queue:add",
            "select:speaker/id:false",
        ]
    );
}

#[tokio::test]
async fn failed_play_stops_the_player_and_deselects_the_output() {
    let _network = NETWORK_TEST_LOCK.lock().await;
    let capture = OwnToneCapture::default();
    capture
        .play_status
        .store(StatusCode::BAD_GATEWAY.as_u16(), Ordering::Relaxed);
    let addr = spawn_owntone(capture.clone()).await;
    let mut sender = on_air_core::sender::airplay::AirPlaySender::new(
        "HomePod",
        "speaker/id",
        format!("http://{addr}"),
    );

    let error = sender.start().await.unwrap_err();
    assert!(error.to_string().contains("player/play"), "{error}");
    assert_eq!(
        *capture.events.lock().unwrap(),
        vec![
            "select:speaker/id:true",
            "/api/player/play",
            "/api/player/stop",
            "select:speaker/id:false",
        ]
    );
}

#[tokio::test]
async fn failed_owntone_stop_is_reported_but_still_deselects_the_output() {
    let _network = NETWORK_TEST_LOCK.lock().await;
    let capture = OwnToneCapture::default();
    capture
        .stop_status
        .store(StatusCode::BAD_GATEWAY.as_u16(), Ordering::Relaxed);
    let addr = spawn_owntone(capture.clone()).await;
    let mut sender = on_air_core::sender::airplay::AirPlaySender::new(
        "HomePod",
        "speaker/id",
        format!("http://{addr}"),
    );

    sender.start().await.unwrap();
    let error = sender.stop().await.unwrap_err();
    assert!(error.to_string().contains("player/stop"), "{error}");
    assert!(capture
        .events
        .lock()
        .unwrap()
        .contains(&"select:speaker/id:false".to_string()));
}
