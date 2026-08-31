use crate::auth::Paired;
use crate::pipeline::{self, capture};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const RING_BUFFER_DURATION_SECONDS: usize = 2;

#[derive(Serialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

pub async fn list_inputs(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<Json<InputsResponse>, StatusCode> {
    if state.mock {
        return Ok(Json(InputsResponse {
            inputs: state.mock_inputs.lock().unwrap().clone(),
        }));
    }
    // CPAL enumeration and the Linux `pactl` fallback are synchronous and may
    // take up to two seconds. Never pin an async request worker while probing.
    let devices = tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        capture::list_input_devices(&host)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(InputsResponse {
        inputs: devices.into_iter().map(|d| d.name).collect(),
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
    let _configuration = state.config_lock.lock().await;
    if state.mock {
        let known = state
            .mock_inputs
            .lock()
            .unwrap()
            .iter()
            .any(|n| n == &req.name);
        if !known {
            return (StatusCode::NOT_FOUND, "input device not found").into_response();
        }
        *state.active_input.lock().unwrap() = Some(req.name);
        return StatusCode::NO_CONTENT.into_response();
    }

    let source_name = if req.name == "__loopback__" {
        let resolved = tokio::task::spawn_blocking(|| {
            let host = cpal::default_host();
            capture::find_preferred_loopback(&host).map(|device| {
                device
                    .map(|device| device.to_string())
                    .or_else(capture::preferred_pulse_monitor)
            })
        })
        .await;
        match resolved {
            Ok(Ok(Some(name))) => name,
            Ok(Ok(None)) => {
                return (StatusCode::NOT_FOUND, "no loopback capture device").into_response()
            }
            Ok(Err(error)) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
            }
            Err(error) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
            }
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

/// Rebuild the active capture pipeline after a sample-rate change. The
/// configuration lock must be held by the caller.
pub(crate) async fn restart_active_capture(state: &CoreState) -> Result<(), (StatusCode, String)> {
    if state.mock {
        return Ok(());
    }
    let Some(name) = state.active_input.lock().unwrap().clone() else {
        return Ok(());
    };

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        tokio::task::spawn_blocking(move || old.stop())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    let handle = start_named_capture(state, name).await?;
    *guard = Some(handle);
    Ok(())
}

async fn start_named_capture(
    state: &CoreState,
    name: String,
) -> Result<pipeline::CaptureHandle, (StatusCode, String)> {
    let target_rate = *state.target_sample_rate_hz.lock().unwrap();
    let ring_capacity = (target_rate as usize).saturating_mul(RING_BUFFER_DURATION_SECONDS);
    let (producer, consumer) = pipeline::new_ring_buffer(ring_capacity);
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
        *state.input_supported_hz.lock().unwrap() = crate::dsp::rates::INPUT_RATES_HZ.to_vec();
        let processing =
            pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
        return Ok(pipeline::CaptureHandle::pulse(pulse, processing, label));
    }

    let started = tokio::task::spawn_blocking(move || {
        let host = cpal::default_host();
        let Some(device) = capture::find_input_device(&host, &name).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let supported_ranges = capture::supported_input_rate_ranges(&device);
        capture::start_capture_at(&device, producer, Some(target_rate))
            .map(|(stream, input_rate)| Some((stream, input_rate, supported_ranges)))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let Some((stream, input_rate, supported_ranges)) =
        started.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    else {
        return Err((StatusCode::NOT_FOUND, "input device not found".into()));
    };
    *state.input_supported_hz.lock().unwrap() =
        crate::dsp::rates::intersect_catalog(&supported_ranges, crate::dsp::rates::INPUT_RATES_HZ);
    let processing =
        pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
    Ok(pipeline::CaptureHandle::cpal(stream, processing, label))
}
