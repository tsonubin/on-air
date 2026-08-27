use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub mod api;
pub mod dsp;
pub mod mdns;
pub mod pairing;
pub mod pipeline;
pub mod sender;
pub mod state;

use state::CoreState;

// Keep in sync with DEFAULT_PORT in packages/api-types/src/index.ts
pub const DEFAULT_PORT: u16 = 47990;

#[derive(Serialize, PartialEq, Debug)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub fn status() -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }
}

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input).get(api::inputs::get_active_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route("/api/outputs/active", post(api::outputs::activate_output).get(api::outputs::get_active_output))
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
        .route("/api/bluetooth/connect", post(api::bluetooth::connect_device))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn status_handler() -> Json<StatusResponse> {
    Json(status())
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
    let _mdns = crate::mdns::spawn_advertisement(DEFAULT_PORT, env!("CARGO_PKG_VERSION"));
    if !state.mock {
        let _discovery = state.spawn_sonos_discovery();
    }
    axum::serve(listener, build_router(state)).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}
