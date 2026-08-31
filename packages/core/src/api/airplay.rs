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
    if state.mock {
        if let Some(device) = state
            .airplay_outputs
            .lock()
            .unwrap()
            .iter_mut()
            .find(|d| d.id == req.device_id)
        {
            device.paired = true;
            device.needs_pair = false;
        }
        return StatusCode::NO_CONTENT;
    }
    let base = state.owntone_base.lock().unwrap().clone();
    match airplay::pair_owntone(&base, &req.device_id, &req.pin).await {
        Ok(()) => {
            if let Some(device) = state
                .airplay_outputs
                .lock()
                .unwrap()
                .iter_mut()
                .find(|d| d.id == req.device_id)
            {
                device.paired = true;
                device.needs_pair = false;
            }
            StatusCode::NO_CONTENT
        }
        Err(_) => StatusCode::BAD_GATEWAY,
    }
}
