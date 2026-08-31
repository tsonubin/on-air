use axum::http::{header, HeaderValue, Method};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

pub mod api;
pub mod auth;
pub mod dsp;
pub mod mdns;
pub mod pairing;
pub mod pipeline;
pub mod sender;
pub mod session;
pub mod state;

use state::CoreState;

// Keep in sync with DEFAULT_PORT in packages/api-types/src/index.ts
pub const DEFAULT_PORT: u16 = 47990;

#[derive(Serialize, PartialEq, Debug)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub service_enabled: bool,
}

pub fn status() -> StatusResponse {
    status_with_service(true)
}

pub fn status_with_service(service_enabled: bool) -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        service_enabled,
    }
}

pub fn build_router(state: CoreState) -> Router {
    let allowed_origins = [
        HeaderValue::from_static("http://127.0.0.1:1420"),
        HeaderValue::from_static("http://localhost:1420"),
        HeaderValue::from_static("tauri://localhost"),
        HeaderValue::from_static("http://tauri.localhost"),
        HeaderValue::from_static("https://tauri.localhost"),
    ];
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route(
            "/api/inputs/active",
            post(api::inputs::activate_input).get(api::inputs::get_active_input),
        )
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route(
            "/api/outputs/active",
            post(api::outputs::activate_output).get(api::outputs::get_active_output),
        )
        .route(
            "/api/outputs/active/volume",
            post(api::outputs::set_output_volume),
        )
        .route("/api/eq", get(api::eq::get_eq).put(api::eq::set_eq))
        .route(
            "/api/sample-rate",
            get(api::sample_rate::get_sample_rate).put(api::sample_rate::set_sample_rate),
        )
        .route("/api/ws", get(api::ws::ws_handler))
        .route("/api/pairing/pin", get(api::pairing::get_pin))
        .route("/api/pairing/verify", post(api::pairing::verify_pin))
        .route("/api/airplay/mode", get(api::airplay::mode))
        .route("/api/airplay/pair", post(api::airplay::pair))
        .route("/api/bluetooth/devices", get(api::bluetooth::list_devices))
        .route("/api/bluetooth/pair", post(api::bluetooth::pair_device))
        .route(
            "/api/bluetooth/connect",
            post(api::bluetooth::connect_device),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(allowed_origins)
                .allow_methods([Method::GET, Method::POST, Method::PUT])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        )
        .with_state(state)
}

async fn status_handler(
    axum::extract::State(state): axum::extract::State<CoreState>,
) -> Json<StatusResponse> {
    Json(status_with_service(
        state.service_enabled.load(Ordering::Acquire),
    ))
}

pub fn mock_mode_enabled() -> bool {
    std::env::var("ON_AIR_MOCK").ok().as_deref() == Some("1")
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let state = if mock_mode_enabled() {
        CoreState::new_mock().await
    } else {
        CoreState::new()
    };
    serve_with_state(listener, state).await
}

pub async fn serve_with_state(listener: TcpListener, state: CoreState) -> std::io::Result<()> {
    let _mdns = crate::mdns::spawn_advertisement(DEFAULT_PORT, env!("CARGO_PKG_VERSION"));
    if !state.mock {
        let _sonos = state.spawn_sonos_discovery();
        let _airplay = state.spawn_airplay_discovery();
    }
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

pub async fn serve_with_state_std(
    listener: std::net::TcpListener,
    state: CoreState,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    serve_with_state(TcpListener::from_std(listener)?, state).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}
