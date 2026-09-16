use crate::auth::LocalClient;
use crate::pairing::VerifyError;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

#[derive(Serialize)]
pub struct PinResponse {
    pub pin: String,
}

pub async fn get_pin(
    LocalClient: LocalClient,
    State(state): State<CoreState>,
) -> Json<PinResponse> {
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
    if !state.service_enabled.load(Ordering::Acquire) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    match state.pairing.lock().unwrap().verify(&req.pin) {
        Ok(token) => Ok(Json(VerifyResponse { token })),
        Err(VerifyError::InvalidPin) => Err(StatusCode::UNAUTHORIZED),
        Err(VerifyError::RateLimited) => Err(StatusCode::TOO_MANY_REQUESTS),
        Err(VerifyError::StorageUnavailable) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
