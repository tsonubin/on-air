use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::dsp::rates;
use crate::session;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
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
    JsonBody(req): JsonBody<SetSampleRateRequest>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    let input_hz = req.input_hz.or(req.sample_rate_hz);
    let output_hz = req.output_hz;
    if input_hz.is_none() && output_hz.is_none() {
        return Err(ApiError::validation(
            "set input_hz, output_hz or sample_rate_hz",
        ));
    }

    let rates = current_sample_rates(&state);
    if let Some(hz) = input_hz {
        if !rates::is_supported(hz, &rates.input.supported_hz) {
            return Err(ApiError::validation(format!(
                "input rate {hz} Hz is not supported by the active input"
            )));
        }
    }
    if let Some(hz) = output_hz {
        if !rates::is_supported(hz, &rates.output.supported_hz) {
            return Err(ApiError::validation(format!(
                "output rate {hz} Hz is not supported by the active output"
            )));
        }
    }

    let old_input_hz = *state.target_sample_rate_hz.lock();
    let old_output_hz = *state.output_sample_rate_hz.lock();
    let new_input_hz = input_hz.unwrap_or(old_input_hz);
    let new_output_hz = output_hz.unwrap_or(old_output_hz);
    let input_changed = new_input_hz != old_input_hz;
    let rates_changed = input_changed || new_output_hz != old_output_hz;
    if !rates_changed {
        return Ok(StatusCode::NO_CONTENT);
    }

    let active_output = state.output().active();
    *state.target_sample_rate_hz.lock() = new_input_hz;
    *state.output_sample_rate_hz.lock() = new_output_hz;

    if input_changed {
        if let Err(error) = state.input().restart().await {
            *state.target_sample_rate_hz.lock() = old_input_hz;
            *state.output_sample_rate_hz.lock() = old_output_hz;
            let restore = state
                .input()
                .restart()
                .await
                .err()
                .map(|e| format!("; restoring previous capture also failed: {e}"))
                .unwrap_or_default();
            return Err(ApiError::internal(format!(
                "could not apply input sample rate: {error}{restore}"
            )));
        }
    }

    if let Some(active) = active_output.as_ref() {
        if let Err(error) = session::activate(&state, &active.transport, &active.device_id).await {
            *state.target_sample_rate_hz.lock() = old_input_hz;
            *state.output_sample_rate_hz.lock() = old_output_hz;
            let capture_restore = if input_changed {
                state
                    .input()
                    .restart()
                    .await
                    .err()
                    .map(|e| format!("; restoring previous capture failed: {e}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let output_restore = session::activate(&state, &active.transport, &active.device_id)
                .await
                .err()
                .map(|e| format!("; restoring previous output failed: {e}"))
                .unwrap_or_default();
            let primary = ApiError::from(error);
            return Err(ApiError::new(
                primary.status(),
                primary.code(),
                format!(
                    "could not restart active output for sample-rate change: {}{capture_restore}{output_restore}",
                    primary.message()
                ),
            ));
        }
    }

    state.remember_sample_rates();
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn current_sample_rates(state: &CoreState) -> SampleRateResponse {
    let input_hz = *state.target_sample_rate_hz.lock();
    let input_supported = supported_input_rates(state);
    let (output_hz, output_supported, transport) = output_side(state);
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
    state.input().supported_hz()
}

fn output_side(state: &CoreState) -> (u32, Vec<u32>, Option<String>) {
    // Read the published snapshot rather than waiting for a connection that
    // can take longer than the remote's polling deadline.
    let snapshot = state.output().snapshot();
    if let (Some(identity), Some(format)) = (snapshot.identity.as_ref(), snapshot.format.as_ref()) {
        return (
            format.sample_rate_hz,
            format.supported_hz.clone(),
            Some(identity.transport.clone()),
        );
    }
    let requested = *state.output_sample_rate_hz.lock();
    match snapshot.identity.as_ref() {
        Some(active) => (
            requested,
            rates::transport_rates(&active.transport).to_vec(),
            Some(active.transport.clone()),
        ),
        None => (requested, vec![44_100, 48_000], None),
    }
}
