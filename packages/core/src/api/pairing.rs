use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct PinResponse {
    pub pin: String,
}

pub async fn get_pin(State(state): State<CoreState>) -> Json<PinResponse> {
    Json(PinResponse {
        pin: state.pairing.lock().unwrap().pin().to_string(),
    })
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub pin: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub token: String,
}

pub async fn verify_pin(
    State(state): State<CoreState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, StatusCode> {
    let token = state
        .pairing
        .lock()
        .unwrap()
        .verify(&req.pin)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(VerifyResponse { token }))
}
