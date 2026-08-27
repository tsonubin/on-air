use crate::auth::Paired;
use crate::sender::bluetooth::BluetoothAdapter;
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

pub async fn list_devices(Paired: Paired, State(state): State<CoreState>) -> Json<BluetoothListResponse> {
    Json(BluetoothListResponse {
        devices: state
            .bluetooth
            .as_ref()
            .list()
            .into_iter()
            .map(|d| BluetoothDeviceInfo {
                id: d.id,
                name: d.name,
                paired: d.paired,
                connected: d.connected,
            })
            .collect(),
    })
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
    state
        .bluetooth
        .pair(&req.id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::NOT_FOUND)
}

pub async fn connect_device(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<BluetoothIdRequest>,
) -> Result<StatusCode, StatusCode> {
    state
        .bluetooth
        .connect(&req.id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::BAD_REQUEST)
}
