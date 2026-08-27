use crate::state::CoreState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    pub transport: &'static str,
}

#[derive(Serialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

pub async fn list_outputs(State(state): State<CoreState>) -> Json<OutputsResponse> {
    let devices = state.outputs.lock().await.list();
    Json(OutputsResponse {
        outputs: devices
            .into_iter()
            .map(|d| OutputInfo {
                id: d.usn,
                name: d.friendly_name,
                transport: "sonos",
            })
            .collect(),
    })
}
