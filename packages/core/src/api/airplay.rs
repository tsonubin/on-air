use crate::api::error::{ApiError, JsonBody};
use crate::auth::Paired;
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

pub async fn mode(Paired: Paired) -> Json<AirPlayModeResponse> {
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
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<AirPlayPairRequest>,
) -> Result<StatusCode, ApiError> {
    let known = state
        .airplay_outputs
        .lock()
        .iter()
        .any(|d| d.id == req.device_id);
    if !known && !state.mock {
        return Err(ApiError::not_found("airplay receiver not found"));
    }
    if req.pin.is_empty() {
        return Err(ApiError::validation("pin must not be empty"));
    }
    if state.mock {
        mark_paired(&state, &req.device_id);
        return Ok(StatusCode::NO_CONTENT);
    }
    let base = state.owntone_base.lock().clone();
    airplay::pair_owntone(&base, &req.device_id, &req.pin).await?;
    mark_paired(&state, &req.device_id);
    Ok(StatusCode::NO_CONTENT)
}

fn mark_paired(state: &CoreState, device_id: &str) {
    if let Some(device) = state
        .airplay_outputs
        .lock()
        .iter_mut()
        .find(|d| d.id == device_id)
    {
        device.paired = true;
        device.needs_pair = false;
    }
}
