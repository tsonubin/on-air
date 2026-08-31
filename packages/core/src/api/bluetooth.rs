use crate::auth::Paired;
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
) -> Result<Json<BluetoothListResponse>, StatusCode> {
    Ok(Json(BluetoothListResponse {
        devices: state
            .bluetooth_devices()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
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
    Json(req): Json<BluetoothIdRequest>,
) -> Result<StatusCode, StatusCode> {
    let adapter = state.bluetooth.clone();
    let paired = tokio::task::spawn_blocking(move || adapter.pair(&req.id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    paired.map_err(|_| StatusCode::NOT_FOUND)?;
    state.invalidate_bluetooth_cache().await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn connect_device(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<BluetoothIdRequest>,
) -> Result<StatusCode, StatusCode> {
    let adapter = state.bluetooth.clone();
    let connected = tokio::task::spawn_blocking(move || adapter.connect(&req.id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    connected.map_err(|_| StatusCode::BAD_REQUEST)?;
    state.invalidate_bluetooth_cache().await;
    Ok(StatusCode::NO_CONTENT)
}
