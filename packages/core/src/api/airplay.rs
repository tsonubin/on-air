use crate::sender::airplay;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct AirPlayModeResponse {
    pub mode: &'static str,
}

pub async fn mode() -> Json<AirPlayModeResponse> {
    Json(AirPlayModeResponse {
        mode: airplay::platform_mode(),
    })
}

#[derive(Deserialize)]
pub struct AirPlayPairRequest {
    pub device_id: String,
    pub pin: String,
}

/// OwnTone/AirPlay2 PIN handshake. Mock mode accepts any PIN for a known device.
pub async fn pair(
    State(state): State<CoreState>,
    Json(req): Json<AirPlayPairRequest>,
) -> StatusCode {
    let known = state
        .airplay_outputs
        .lock()
        .unwrap()
        .iter()
        .any(|d| d.id == req.device_id);
    if !known && !state.mock {
        return StatusCode::NOT_FOUND;
    }
    if req.pin.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    StatusCode::NO_CONTENT
}
