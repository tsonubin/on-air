use crate::auth::Paired;
use crate::session::{self, ActivateError};
use crate::state::{ActiveOutput, CoreState};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    pub transport: &'static str,
    pub kind: &'static str,
    pub member_count: u8,
    pub needs_pair: bool,
    pub paired: bool,
}

#[derive(Serialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

pub async fn list_outputs(Paired: Paired, State(state): State<CoreState>) -> Json<OutputsResponse> {
    Json(OutputsResponse {
        outputs: session::list(&state)
            .await
            .into_iter()
            .map(|o| OutputInfo {
                id: o.id,
                name: o.name,
                transport: o.transport,
                kind: o.kind,
                member_count: o.member_count,
                needs_pair: o.needs_pair,
                paired: o.paired,
            })
            .collect(),
    })
}

pub async fn get_active_output(Paired: Paired, State(state): State<CoreState>) -> Json<Option<ActiveOutput>> {
    Json(state.active_output.lock().unwrap().clone())
}

#[derive(Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

pub async fn activate_output(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<ActivateOutputRequest>,
) -> Response {
    match session::activate(&state, &req.transport, &req.device_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(ActivateError::NotFound) => (StatusCode::NOT_FOUND, "output device not found").into_response(),
        Err(ActivateError::BadRequest(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
        Err(ActivateError::Failed(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

pub async fn set_output_volume(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<SetVolumeRequest>,
) -> Response {
    match session::set_volume(&state, req.volume).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(ActivateError::Failed(msg)) if msg == "no active output" => {
            (StatusCode::CONFLICT, "no active output").into_response()
        }
        Err(ActivateError::Failed(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(ActivateError::NotFound) => (StatusCode::NOT_FOUND, "output device not found").into_response(),
        Err(ActivateError::BadRequest(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
    }
}
