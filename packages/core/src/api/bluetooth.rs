use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct BluetoothDeviceInfo {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Serialize)]
pub struct BluetoothListResponse {
    pub devices: Vec<BluetoothDeviceInfo>,
}

pub async fn list_devices(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<Json<BluetoothListResponse>, ApiError> {
    Ok(Json(BluetoothListResponse {
        devices: state
            .bluetooth_devices()
            .await
            .map_err(ApiError::internal)?
            .into_iter()
            .map(|d| BluetoothDeviceInfo {
                id: d.id,
                name: d.name,
                paired: d.paired,
                connected: d.connected,
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
pub struct BluetoothIdRequest {
    pub id: String,
}

pub async fn pair_device(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<BluetoothIdRequest>,
) -> Result<StatusCode, ApiError> {
    let adapter = state.bluetooth.clone();
    tokio::task::spawn_blocking(move || adapter.pair(&req.id))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))??;
    state.invalidate_bluetooth_cache().await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn connect_device(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<BluetoothIdRequest>,
) -> Result<StatusCode, ApiError> {
    let adapter = state.bluetooth.clone();
    tokio::task::spawn_blocking(move || adapter.connect(&req.id))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))??;
    state.invalidate_bluetooth_cache().await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn open_settings(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<StatusCode, ApiError> {
    let adapter = state.bluetooth.clone();
    tokio::task::spawn_blocking(move || adapter.open_settings())
        .await
        .map_err(|error| ApiError::internal(error.to_string()))??;
    state.invalidate_bluetooth_cache().await;
    Ok(StatusCode::NO_CONTENT)
}
