use crate::api::auth::{Authorized, Paired};
use crate::api::error::{ApiError, JsonBody};
use crate::state::CoreState;
use axum::{extract::State, Json};
use on_air_api::{PatchbayCommand, PatchbaySnapshot};
use std::sync::atomic::Ordering;

pub async fn snapshot(
    Authorized: Authorized,
    State(state): State<CoreState>,
) -> Json<PatchbaySnapshot> {
    let first = !state.patchbay.activated.swap(true, Ordering::AcqRel);
    let previous = state.patchbay.snapshot().await;
    if first || (previous.nodes.is_empty() && previous.error.is_none()) {
        state.patchbay.refresh(&state).await;
    }
    Json(state.patchbay.snapshot().await)
}

pub async fn command(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(command): JsonBody<PatchbayCommand>,
) -> Result<Json<PatchbaySnapshot>, ApiError> {
    state
        .patchbay
        .command(&state, command)
        .await
        .map(Json)
        .map_err(ApiError::validation)
}
