use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::cd::AUDIO_CD_INPUT;
use crate::pipeline::capture;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

pub use crate::pipeline::{InputError, LOOPBACK_INPUT};

#[derive(Serialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

fn with_cd_input(state: &CoreState, mut inputs: Vec<String>) -> Vec<String> {
    if state.cd.status().present && !inputs.iter().any(|name| name == AUDIO_CD_INPUT) {
        inputs.insert(0, AUDIO_CD_INPUT.to_string());
    }
    inputs
}

pub async fn list_inputs(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<Json<InputsResponse>, ApiError> {
    if state.mock {
        return Ok(Json(InputsResponse {
            inputs: with_cd_input(&state, state.input().mock_inputs()),
        }));
    }
    // CPAL enumeration and the Linux `pactl` fallback are synchronous and may
    // take up to two seconds. Never pin an async request worker while probing.
    let devices = tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        capture::list_input_devices(&host)
    })
    .await
    .map_err(|error| ApiError::internal(error.to_string()))?
    .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(InputsResponse {
        inputs: with_cd_input(&state, devices.into_iter().map(|d| d.name).collect()),
    }))
}

#[derive(Serialize)]
pub struct ActiveInputResponse {
    pub name: Option<String>,
    pub backend: &'static str,
}

pub async fn get_active_input(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<ActiveInputResponse> {
    Json(ActiveInputResponse {
        name: state.input().active_name(),
        backend: capture::loopback_backend(),
    })
}

#[derive(Deserialize)]
pub struct ActivateInputRequest {
    pub name: String,
}

pub async fn activate_input(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<ActivateInputRequest>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    state.activate_input_locked(&req.name).await?;
    Ok(StatusCode::NO_CONTENT)
}
