use crate::api::error::{ApiError, JsonBody};
use crate::auth::Paired;
use crate::session::{self, OutputInfo};
use crate::state::{ActiveOutput, CoreState};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

#[derive(Serialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

pub async fn list_outputs(Paired: Paired, State(state): State<CoreState>) -> Json<OutputsResponse> {
    Json(OutputsResponse {
        outputs: session::list(&state).await,
    })
}

#[derive(Serialize)]
pub struct ActiveOutputResponse {
    pub active: Option<ActiveOutput>,
}

pub async fn get_active_output(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<ActiveOutputResponse> {
    Json(ActiveOutputResponse {
        active: state.active_output.lock().clone(),
    })
}

#[derive(Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

pub async fn activate_output(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<ActivateOutputRequest>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    session::activate(&state, &req.transport, &req.device_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/outputs/active`: stop casting and forget the saved output so
/// the next launch does not resume it.
pub async fn deactivate_output(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    state.deactivate_sender().await?;
    state.clear_saved_output();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

#[derive(Serialize)]
pub struct OutputVolumeResponse {
    pub volume: u8,
}

pub async fn get_output_volume(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<OutputVolumeResponse> {
    Json(OutputVolumeResponse {
        volume: state.output_volume.load(Ordering::Acquire),
    })
}

pub async fn set_output_volume(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<SetVolumeRequest>,
) -> Result<StatusCode, ApiError> {
    if req.volume > 100 {
        return Err(ApiError::validation("volume must be between 0 and 100"));
    }
    session::set_volume(&state, req.volume).await?;
    Ok(StatusCode::NO_CONTENT)
}
