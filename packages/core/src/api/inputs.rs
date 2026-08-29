use crate::auth::Paired;
use crate::pipeline::{self, capture};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const RING_BUFFER_CAPACITY_FRAMES: usize = 1024 * 8;

#[derive(Serialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

pub async fn list_inputs(Paired: Paired, State(state): State<CoreState>) -> Result<Json<InputsResponse>, StatusCode> {
    if state.mock {
        return Ok(Json(InputsResponse {
            inputs: state.mock_inputs.lock().unwrap().clone(),
        }));
    }
    let host = cpal::default_host();
    let devices =
        capture::list_input_devices(&host).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(InputsResponse {
        inputs: devices.into_iter().map(|d| d.name).collect(),
    }))
}

#[derive(Serialize)]
pub struct ActiveInputResponse {
    pub name: Option<String>,
    pub backend: &'static str,
}

pub async fn get_active_input(Paired: Paired, State(state): State<CoreState>) -> Json<ActiveInputResponse> {
    Json(ActiveInputResponse {
        name: state.active_input.lock().unwrap().clone(),
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
    Json(req): Json<ActivateInputRequest>,
) -> Response {
    if state.mock {
        let known = state.mock_inputs.lock().unwrap().iter().any(|n| n == &req.name);
        if !known {
            return (StatusCode::NOT_FOUND, "input device not found").into_response();
        }
        *state.active_input.lock().unwrap() = Some(req.name);
        return StatusCode::NO_CONTENT.into_response();
    }

    let source_name = if req.name == "__loopback__" {
        let host = cpal::default_host();
        match capture::find_preferred_loopback(&host) {
            Ok(Some(d)) => d.to_string(),
            Ok(None) => match capture::preferred_pulse_monitor() {
                Some(name) => name,
                None => {
                    return (StatusCode::NOT_FOUND, "no loopback capture device").into_response()
                }
            },
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        req.name.clone()
    };

    let new_handle = match start_named_capture(&state, source_name.clone()).await {
        Ok(handle) => handle,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *guard = Some(new_handle);
    *state.active_input.lock().unwrap() = Some(source_name);

    StatusCode::NO_CONTENT.into_response()
}

async fn start_named_capture(
    state: &CoreState,
    name: String,
) -> Result<pipeline::CaptureHandle, (StatusCode, String)> {
    let target_rate = *state.target_sample_rate_hz.lock().unwrap();
    let (producer, consumer) = pipeline::new_ring_buffer(RING_BUFFER_CAPACITY_FRAMES);
    let eq = state.eq_gains_db.clone();
    let audio_tx = state.audio_tx.clone();
    let ws_tx = state.ws_tx.clone();
    let label = name.clone();

    if capture::is_pulse_monitor_name(&name) {
        let src = name.clone();
        let started = tokio::task::spawn_blocking(move || {
            capture::start_pulse_monitor(&src, producer, Some(target_rate))
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let (pulse, input_rate) = started.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let processing =
            pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
        return Ok(pipeline::CaptureHandle::pulse(pulse, processing, label));
    }

    let host = cpal::default_host();
    let device = capture::find_input_device(&host, &name)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "input device not found".into()))?;
    let started = tokio::task::spawn_blocking(move || {
        capture::start_capture_at(&device, producer, Some(target_rate))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (stream, input_rate) =
        started.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let processing =
        pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
    Ok(pipeline::CaptureHandle::cpal(stream, processing, label))
}
