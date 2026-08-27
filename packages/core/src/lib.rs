use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub mod api;
pub mod dsp;
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
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route("/api/outputs/active", post(api::outputs::activate_output))
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
        .with_state(state)
}

async fn status_handler() -> Json<StatusResponse> {
    Json(status())
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, build_router(CoreState::new())).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}
