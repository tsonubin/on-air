use crate::auth::Paired;
use crate::dsp::rates;
use crate::session;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SampleRateSide {
    pub sample_rate_hz: u32,
    pub supported_hz: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

#[derive(Serialize)]
pub struct SampleRateResponse {
    /// Pipeline / input rate. Kept for older clients.
    pub sample_rate_hz: u32,
    pub input: SampleRateSide,
    pub output: SampleRateSide,
}

pub async fn get_sample_rate(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<SampleRateResponse> {
    Json(current_sample_rates(&state))
}

#[derive(Deserialize)]
pub struct SetSampleRateRequest {
    pub sample_rate_hz: Option<u32>,
    pub input_hz: Option<u32>,
    pub output_hz: Option<u32>,
}

pub async fn set_sample_rate(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<SetSampleRateRequest>,
) -> Response {
    let _configuration = state.config_lock.lock().await;
    let input_hz = req.input_hz.or(req.sample_rate_hz);
    let output_hz = req.output_hz;
    if input_hz.is_none() && output_hz.is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let rates = current_sample_rates(&state);
    if let Some(hz) = input_hz {
        if !rates::is_supported(hz, &rates.input.supported_hz) {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    if let Some(hz) = output_hz {
        if !rates::is_supported(hz, &rates.output.supported_hz) {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }

    let old_input_hz = *state.target_sample_rate_hz.lock().unwrap();
    let old_output_hz = *state.output_sample_rate_hz.lock().unwrap();
    let new_input_hz = input_hz.unwrap_or(old_input_hz);
    let new_output_hz = output_hz.unwrap_or(old_output_hz);
    let input_changed = new_input_hz != old_input_hz;
    let rates_changed = input_changed || new_output_hz != old_output_hz;
    if !rates_changed {
        return StatusCode::NO_CONTENT.into_response();
    }

    let active_output = state.active_output.lock().unwrap().clone();
    *state.target_sample_rate_hz.lock().unwrap() = new_input_hz;
    *state.output_sample_rate_hz.lock().unwrap() = new_output_hz;

    if input_changed {
        if let Err((_, error)) = crate::api::inputs::restart_active_capture(&state).await {
            *state.target_sample_rate_hz.lock().unwrap() = old_input_hz;
            *state.output_sample_rate_hz.lock().unwrap() = old_output_hz;
            let restore = crate::api::inputs::restart_active_capture(&state)
                .await
                .err()
                .map(|(_, e)| format!("; restoring previous capture also failed: {e}"))
                .unwrap_or_default();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not apply input sample rate: {error}{restore}"),
            )
                .into_response();
        }
    }

    if let Some(active) = active_output.as_ref() {
        if let Err(error) = session::activate(&state, &active.transport, &active.device_id).await {
            *state.target_sample_rate_hz.lock().unwrap() = old_input_hz;
            *state.output_sample_rate_hz.lock().unwrap() = old_output_hz;
            let capture_restore = if input_changed {
                crate::api::inputs::restart_active_capture(&state)
                    .await
                    .err()
                    .map(|(_, e)| format!("; restoring previous capture failed: {e}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let output_restore = session::activate(&state, &active.transport, &active.device_id)
                .await
                .err()
                .map(|e| format!("; restoring previous output failed: {e:?}"))
                .unwrap_or_default();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "could not restart active output for sample-rate change: {error:?}{capture_restore}{output_restore}"
                ),
            )
                .into_response();
        }
    }

    state.remember_sample_rates();
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) fn current_sample_rates(state: &CoreState) -> SampleRateResponse {
    let input_hz = *state.target_sample_rate_hz.lock().unwrap();
    let output_hz = *state.output_sample_rate_hz.lock().unwrap();
    let input_supported = supported_input_rates(state);
    let (output_supported, transport) = supported_output_side(state);
    SampleRateResponse {
        sample_rate_hz: input_hz,
        input: SampleRateSide {
            sample_rate_hz: input_hz,
            supported_hz: input_supported,
            transport: None,
        },
        output: SampleRateSide {
            sample_rate_hz: output_hz,
            supported_hz: output_supported,
            transport,
        },
    }
}

fn supported_input_rates(state: &CoreState) -> Vec<u32> {
    state.input_supported_hz.lock().unwrap().clone()
}

fn supported_output_side(state: &CoreState) -> (Vec<u32>, Option<String>) {
    let active = state.active_output.lock().unwrap().clone();
    match active {
        Some(active) => (
            crate::session::supported_output_rates(
                state,
                &active.transport,
                Some(&active.device_id),
            ),
            Some(active.transport),
        ),
        None => (vec![44_100, 48_000], None),
    }
}
