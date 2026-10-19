//! On-demand audio measurements, explicitly applied capture buffering and
//! DSP quality, and the first-run tuning status.
use crate::api::{
    auth::Paired,
    error::{ApiError, JsonBody},
};
use crate::audio_benchmark::{run, BUFFER_OPTIONS_MS};
use crate::dsp::DspQuality;
use crate::settings::FirstRunTuning;
use crate::state::{ApplyError, CoreState};
use axum::{extract::State, http::StatusCode, Json};
use on_air_api::{AudioBenchmark, SetCaptureBufferRequest};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

/// `PUT /api/audio/quality`: the user's explicit DSP profile choice. It
/// always wins over the first-run tuning's recommendation. The request
/// shape is local to the core: the shared wire crate is outside this
/// change's slice and clients mirror it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetDspQualityRequest {
    pub quality: DspQuality,
}

/// `GET /api/audio/tuning`: whether an audio check has completed on this
/// machine, what the automatic first-run tuning measured and applied, and
/// the profile currently in effect. Local to the core (see
/// [`SetDspQualityRequest`]); the TUI mirrors the shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuningStatus {
    /// Whether any audio check has completed here (automatic or manual).
    /// `false` means first-run tuning is still pending.
    pub tuned: bool,
    /// What the automatic first-run tuning measured and applied; `None`
    /// until it has completed.
    pub automatic: Option<FirstRunTuning>,
    /// The profile currently in effect.
    pub quality: DspQuality,
    /// The user's explicit profile choice, if any.
    pub quality_override: Option<DspQuality>,
    /// The most recent check's quality verdict, if any.
    pub recommended_quality: Option<DspQuality>,
    /// The capture buffer currently requested (0 = system default).
    pub capture_buffer_ms: u32,
    /// All selectable profiles, cheapest first.
    pub profiles: Vec<DspQuality>,
}

fn apply_error(error: ApplyError) -> ApiError {
    match error {
        ApplyError::Paused => ApiError::service_paused(),
        ApplyError::Failed(message) => ApiError::internal(message),
    }
}

pub async fn benchmark(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<Json<AudioBenchmark>, ApiError> {
    let guard = state
        .audio_benchmark_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| ApiError::conflict("an audio check is already running"))?;
    let input = state.input_sample_rate_hz();
    let output = state.output_sample_rate_hz();
    let gains = state.eq_gains_db();
    let quality = state.dsp_quality();
    let outcome = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        run(input, output, gains, 96, quality)
    })
    .await
    .map_err(|_| ApiError::internal("audio check stopped unexpectedly"))?
    .map_err(ApiError::internal)?;
    // A completed check marks this machine as tuned (non-destructive:
    // nothing is applied from the manual button).
    state.record_check_verdict(outcome.recommended_quality);
    Ok(Json(outcome.report))
}

pub async fn tuning_status(Paired: Paired, State(state): State<CoreState>) -> Json<TuningStatus> {
    let saved = state.saved_settings();
    Json(TuningStatus {
        tuned: saved.recommended_dsp_quality.is_some(),
        automatic: saved.first_run_tuning.clone(),
        quality: state.dsp_quality(),
        quality_override: saved.dsp_quality,
        recommended_quality: saved.recommended_dsp_quality,
        capture_buffer_ms: state.capture_buffer_ms.load(Ordering::Acquire),
        profiles: DspQuality::ALL.to_vec(),
    })
}

pub async fn set_buffer(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(request): JsonBody<SetCaptureBufferRequest>,
) -> Result<StatusCode, ApiError> {
    if !BUFFER_OPTIONS_MS.contains(&request.buffer_ms) {
        return Err(ApiError::validation(
            "capture buffer must be default (0), 50, 100, 200 or 400 ms",
        ));
    }
    tokio::spawn(
        async move { apply_error_result(state.apply_capture_buffer(request.buffer_ms).await) },
    )
    .await
    .map_err(|_| ApiError::internal("capture buffer change stopped unexpectedly"))?
}

pub async fn set_quality(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(request): JsonBody<SetDspQualityRequest>,
) -> Result<StatusCode, ApiError> {
    tokio::spawn(async move { apply_error_result(state.apply_dsp_quality(request.quality).await) })
        .await
        .map_err(|_| ApiError::internal("DSP quality change stopped unexpectedly"))?
}

fn apply_error_result(result: Result<(), ApplyError>) -> Result<StatusCode, ApiError> {
    result.map(|()| StatusCode::NO_CONTENT).map_err(apply_error)
}
