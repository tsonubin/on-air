//! Per-output sync status and the user's latency trim (`/api/sync`).

use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::api::sample_rate::current_sample_rates;
use crate::state::CoreState;
use crate::sync::latency::TRIM_LIMIT_MS;
use crate::sync::OutputKey;
use axum::extract::State;
use axum::Json;

pub use on_air_api::{SetSyncTrimRequest, SyncResponse, SyncTrimResponse};

/// `GET /api/sync`: every output's latency model, delay line, measured
/// drift and coarse sync state. Latency figures are documented defaults
/// plus the user's trim — never measurements.
pub async fn get_sync(Paired: Paired, State(state): State<CoreState>) -> Json<SyncResponse> {
    let rate = current_sample_rates(&state).output.sample_rate_hz;
    let live: Vec<_> = state
        .output()
        .snapshot()
        .identity
        .into_iter()
        .map(|identity| (identity, rate))
        .collect();
    Json(SyncResponse {
        outputs: state.sync.sync_status(&live),
    })
}

/// `PUT /api/sync/trim`: manually calibrate one output's latency in
/// milliseconds against what the user actually hears. The change applies
/// the next time that output's feed opens (its next activation or stream).
pub async fn set_sync_trim(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<SetSyncTrimRequest>,
) -> Result<Json<SyncTrimResponse>, ApiError> {
    if req.trim_ms.abs() > TRIM_LIMIT_MS {
        return Err(ApiError::validation(format!(
            "trim_ms must be between -{TRIM_LIMIT_MS} and {TRIM_LIMIT_MS}"
        )));
    }
    let known = req.transport == crate::sender::local::TRANSPORT
        || on_air_api::transport::ALL.contains(&req.transport.as_str());
    if !known {
        return Err(ApiError::validation(format!(
            "unknown transport: {}",
            req.transport
        )));
    }
    let key = OutputKey::new(&req.transport, &req.device_id);
    let estimate = state.sync.set_trim_ms(&key, &req.transport, req.trim_ms);
    state.remember_sync_trim(&req.transport, &req.device_id, req.trim_ms);
    Ok(Json(SyncTrimResponse {
        trim_ms: req.trim_ms,
        effective_latency_ms: estimate.latency.as_millis() as u32,
    }))
}
