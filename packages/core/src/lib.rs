use axum::{routing::get, Json, Router};
use serde::Serialize;

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

pub fn build_router() -> Router {
    Router::new().route("/api/status", get(status_handler))
}

async fn status_handler() -> Json<StatusResponse> {
    Json(status())
}
