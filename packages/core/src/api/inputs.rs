use crate::auth::Paired;
use crate::pipeline::{self, capture};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cpal::traits::DeviceTrait;
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

    let host = cpal::default_host();
    let device = if req.name == "__loopback__" {
        match capture::find_preferred_loopback(&host) {
            Ok(Some(d)) => d,
            Ok(None) => return (StatusCode::NOT_FOUND, "no loopback capture device").into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match capture::find_input_device(&host, &req.name) {
            Ok(Some(d)) => d,
            Ok(None) => return (StatusCode::NOT_FOUND, "input device not found").into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    };

    let target_rate = *state.target_sample_rate_hz.lock().unwrap();
    let input_rate = device
        .default_input_config()
        .map(|c| c.sample_rate())
        .unwrap_or(target_rate);

    let (producer, consumer) = pipeline::new_ring_buffer(RING_BUFFER_CAPACITY_FRAMES);
    let started = tokio::task::spawn_blocking(move || capture::start_capture(&device, producer))
        .await;
    let stream = match started {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let processing = pipeline::spawn_processing_task(
        consumer,
        input_rate,
        target_rate,
        state.eq_gains_db.clone(),
        state.audio_tx.clone(),
        state.ws_tx.clone(),
    );

    let new_handle = pipeline::CaptureHandle {
        stream,
        processing,
        device_name: req.name.clone(),
    };

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *guard = Some(new_handle);
    *state.active_input.lock().unwrap() = Some(req.name);

    StatusCode::NO_CONTENT.into_response()
}
