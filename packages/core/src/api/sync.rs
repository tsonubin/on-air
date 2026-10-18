//! Per-output sync status, the user's latency trim, and the sync-group
//! lifecycle (`/api/sync`, `/api/sync/groups`).
//!
//! `GET /api/sync` is the one read model for sync: every output's latency
//! model, delay line, drift and coarse state, plus every named sync group
//! with its members (several speakers that play together — not one merged
//! device). The `/api/sync/groups` routes mutate that model; each change
//! broadcasts `SyncGroupChanged` so clients re-read the one response.

use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::api::sample_rate::current_sample_rates;
use crate::state::CoreState;
use crate::sync::latency::TRIM_LIMIT_MS;
use crate::sync::lifecycle::{self, GroupError};
use crate::sync::OutputKey;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

pub use on_air_api::{
    CreateSyncGroupRequest, SetSyncTrimRequest, SyncGroupMemberRequest, SyncGroupView, SyncResponse,
    SyncTrimResponse,
};

/// `GET /api/sync`: every output's latency model, delay line, measured
/// drift and coarse sync state, and every sync group. Latency figures are
/// documented defaults plus the user's trim — never measurements.
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
        groups: Some(state.sync.group_views()),
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

/// `POST /api/sync/groups`: create an empty named sync group.
pub async fn create_sync_group(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<CreateSyncGroupRequest>,
) -> Result<Json<SyncGroupView>, ApiError> {
    let id = lifecycle::create(&state, &req.name)
        .await
        .map_err(group_error)?;
    let view = state
        .sync
        .group_views()
        .into_iter()
        .find(|group| group.id == id)
        .expect("the group was just created");
    Ok(Json(view))
}

/// `DELETE /api/sync/groups/{id}`: dissolve a group. Its playing members
/// stop cleanly; free-running outputs are untouched.
pub async fn dissolve_sync_group(
    Paired: Paired,
    State(state): State<CoreState>,
    Path(group_id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lifecycle::dissolve(&state, group_id)
        .await
        .map_err(group_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/sync/groups/{id}/members`: put one output into a group. The
/// output keeps playing as it is; the membership takes effect at the
/// group's next `start`. An output that already plays in ANOTHER group is
/// refused.
pub async fn add_sync_group_member(
    Paired: Paired,
    State(state): State<CoreState>,
    Path(group_id): Path<u64>,
    JsonBody(req): JsonBody<SyncGroupMemberRequest>,
) -> Result<StatusCode, ApiError> {
    lifecycle::add_member(&state, group_id, &req.transport, &req.device_id)
        .await
        .map_err(group_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/sync/groups/{id}/leave`: take one output out of a group. If
/// it is playing with the group it stops cleanly and the other members keep
/// playing.
pub async fn leave_sync_group_member(
    Paired: Paired,
    State(state): State<CoreState>,
    Path(group_id): Path<u64>,
    JsonBody(req): JsonBody<SyncGroupMemberRequest>,
) -> Result<StatusCode, ApiError> {
    lifecycle::remove_member(&state, group_id, &req.transport, &req.device_id)
        .await
        .map_err(group_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/sync/groups/{id}/start`: begin every member at one
/// presentation epoch. A member playing free-running is taken over; a
/// receiver another session drives is refused and nothing starts.
pub async fn start_sync_group(
    Paired: Paired,
    State(state): State<CoreState>,
    Path(group_id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lifecycle::start(&state, group_id).await.map_err(|error| {
        let GroupError::StartFailed(failures) = error else {
            return group_error(error);
        };
        // Starting is per member: name the ones that failed and say
        // plainly when the others are already playing.
        let total = state
            .sync
            .group_by_id(group_id)
            .map(|group| group.members().len())
            .unwrap_or(0);
        let others = if failures.len() < total {
            " (the other members are playing; stop the group to try again)"
        } else {
            ""
        };
        ApiError::transport_unreachable(format!("{}{others}", failures.join("; ")))
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/sync/groups/{id}/stop`: stop every member cleanly. Membership
/// stays; the group is simply no longer playing.
pub async fn stop_sync_group(
    Paired: Paired,
    State(state): State<CoreState>,
    Path(group_id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lifecycle::stop(&state, group_id)
        .await
        .map_err(group_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The one error envelope for the group lifecycle.
fn group_error(error: GroupError) -> ApiError {
    match error {
        GroupError::NotFound => ApiError::not_found(error.to_string()),
        GroupError::OutputNotFound => ApiError::not_found(error.to_string()),
        GroupError::NotAMember => ApiError::not_found(error.to_string()),
        GroupError::InvalidName => ApiError::validation(error.to_string()),
        GroupError::Empty => ApiError::validation(error.to_string()),
        GroupError::AlreadyInGroup { .. } => ApiError::conflict(error.to_string()),
        GroupError::StopFailed { .. } => ApiError::conflict(error.to_string()),
        GroupError::Busy(_) => ApiError::not_ready(error.to_string()),
        GroupError::Paused => ApiError::service_paused(),
        GroupError::StartFailed(_) => ApiError::transport_unreachable(error.to_string()),
    }
}
