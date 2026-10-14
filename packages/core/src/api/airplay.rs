use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::sender::airplay;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

pub use on_air_api::{AirPlayModeResponse, AirPlayPairRequest};

pub async fn mode(Paired: Paired) -> Json<AirPlayModeResponse> {
    Json(AirPlayModeResponse {
        mode: airplay::platform_mode().into(),
    })
}

/// OwnTone/AirPlay2 PIN handshake. Mock mode accepts any PIN for a known device.
pub async fn pair(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<AirPlayPairRequest>,
) -> Result<StatusCode, ApiError> {
    let known = state.airplay.lock().iter().any(|d| d.id == req.device_id);
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
    if let Some(device) = state.airplay.lock().iter_mut().find(|d| d.id == device_id) {
        device.paired = true;
        device.needs_pair = false;
    }
}
