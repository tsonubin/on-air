//! Acoustic latency calibration over the API (roadmap B10):
//! `POST /api/sync/calibrate` triggers a run, `GET /api/sync/calibration`
//! monitors it, and the applied results show up in `GET /api/sync` as
//! `calibration` / `calibrated: true` alongside the defaults they replaced.
//!
//! The run replies immediately (202) and continues in the background:
//! measuring four transports with three probe runs each takes minutes, not
//! milliseconds, and a dropped HTTP request must not strand a run. Every
//! milestone broadcasts `WsEvent::CalibrationChanged` so clients re-read.

use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::calibration::run::{self, MockLoopback, StartError};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use on_air_api::{CalibrationRunView, CalibrationStatusResponse, StartCalibrationRequest};

/// `POST /api/sync/calibrate`: start measuring. Each target is driven in
/// turn through its existing transport; the probe plays on the pipeline
/// PCM bus and the given microphone decides when it arrived.
pub async fn start_calibration(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(request): JsonBody<StartCalibrationRequest>,
) -> Result<(StatusCode, Json<CalibrationRunView>), ApiError> {
    let view = run::start(&state, request).await.map_err(start_error)?;
    Ok((StatusCode::ACCEPTED, Json(view)))
}

/// `GET /api/sync/calibration`: the run in progress, if any, and the most
/// recently finished run (including low-confidence attempts that were
/// reported and not applied).
pub async fn calibration_status(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<CalibrationStatusResponse> {
    Json(state.calibration.status())
}

fn start_error(error: StartError) -> ApiError {
    match error {
        StartError::Busy => ApiError::conflict(error.to_string()),
        StartError::Invalid(_) => ApiError::validation(error.to_string()),
    }
}

/// `POST /api/mock/calibration/loopback` (mock core only): configure the
/// synthetic acoustic world the CI harness measures against — the ground
/// truth behind the probe. This route does not exist on a production core;
/// a real run always goes through real hardware.
pub async fn simulate_loopback(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(world): JsonBody<MockLoopback>,
) -> StatusCode {
    state.calibration.set_mock_loopback(Some(world));
    StatusCode::NO_CONTENT
}
