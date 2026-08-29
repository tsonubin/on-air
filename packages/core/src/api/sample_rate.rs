use crate::auth::Paired;
use crate::dsp::rates::{self, INPUT_RATES_HZ};
use crate::pipeline::capture;
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

pub async fn get_sample_rate(Paired: Paired, State(state): State<CoreState>) -> Json<SampleRateResponse> {
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
) -> StatusCode {
    let input_hz = req.input_hz.or(req.sample_rate_hz);
    let output_hz = req.output_hz;
    if input_hz.is_none() && output_hz.is_none() {
        return StatusCode::BAD_REQUEST;
    }

    let rates = current_sample_rates(&state);
    if let Some(hz) = input_hz {
        if !rates::is_supported(hz, &rates.input.supported_hz) {
            return StatusCode::BAD_REQUEST;
        }
        *state.target_sample_rate_hz.lock().unwrap() = hz;
    }
    if let Some(hz) = output_hz {
        if !rates::is_supported(hz, &rates.output.supported_hz) {
            return StatusCode::BAD_REQUEST;
        }
        *state.output_sample_rate_hz.lock().unwrap() = hz;
    }
    StatusCode::NO_CONTENT
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
    if state.mock {
        return INPUT_RATES_HZ.to_vec();
    }
    let active = state.active_input.lock().unwrap().clone();
    let Some(name) = active else {
        return INPUT_RATES_HZ.to_vec();
    };
    let ranges = capture::supported_input_rates_for_name(&name);
    rates::intersect_catalog(&ranges, INPUT_RATES_HZ)
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
