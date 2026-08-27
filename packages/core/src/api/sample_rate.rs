use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SampleRateResponse {
    pub sample_rate_hz: u32,
}

pub async fn get_sample_rate(State(state): State<CoreState>) -> Json<SampleRateResponse> {
    Json(SampleRateResponse {
        sample_rate_hz: *state.target_sample_rate_hz.lock().unwrap(),
    })
}

#[derive(Deserialize)]
pub struct SetSampleRateRequest {
    pub sample_rate_hz: u32,
}

pub async fn set_sample_rate(
    State(state): State<CoreState>,
    Json(req): Json<SetSampleRateRequest>,
) -> StatusCode {
    *state.target_sample_rate_hz.lock().unwrap() = req.sample_rate_hz;
    StatusCode::NO_CONTENT
}
