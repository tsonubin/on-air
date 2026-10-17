//! On-demand audio measurements and explicitly applied capture buffering.
use crate::api::{
    auth::Paired,
    error::{ApiError, JsonBody},
};
use crate::audio_benchmark::{run, BUFFER_OPTIONS_MS};
use crate::state::CoreState;
use axum::{extract::State, http::StatusCode, Json};
use on_air_api::{AudioBenchmark, SetCaptureBufferRequest};
use std::sync::atomic::Ordering;

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
    let gains = *state.eq_gains_db.lock();
    let report = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        run(input, output, gains, 96)
    })
    .await
    .map_err(|_| ApiError::internal("audio check stopped unexpectedly"))?
    .map_err(ApiError::internal)?;
    Ok(Json(report))
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
    tokio::spawn(async move { apply_buffer(state, request.buffer_ms).await })
        .await
        .map_err(|_| ApiError::internal("capture buffer change stopped unexpectedly"))?
}

async fn apply_buffer(state: CoreState, buffer_ms: u32) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    if !state.service_enabled.load(Ordering::Acquire) {
        return Err(ApiError::service_paused());
    }
    let previous = state.capture_buffer_ms.swap(buffer_ms, Ordering::AcqRel);
    if previous == buffer_ms {
        return Ok(StatusCode::NO_CONTENT);
    }
    if let Err(error) = state.input().restart().await {
        state.capture_buffer_ms.store(previous, Ordering::Release);
        let restore = state
            .input()
            .restart()
            .await
            .err()
            .map(|e| format!("; restoring capture also failed: {e}"))
            .unwrap_or_default();
        return Err(ApiError::internal(format!(
            "could not apply capture buffer: {error}{restore}"
        )));
    }
    state.remember_capture_buffer(buffer_ms);
    Ok(StatusCode::NO_CONTENT)
}
