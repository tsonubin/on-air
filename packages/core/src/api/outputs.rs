use crate::sender::sonos::{net::local_lan_ip, SonosSender};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

pub async fn activate_output(
    State(state): State<CoreState>,
    Json(req): Json<ActivateOutputRequest>,
) -> Response {
    if req.transport != "sonos" {
        return (StatusCode::BAD_REQUEST, "only 'sonos' is supported in M1").into_response();
    }

    let device = {
        let registry = state.outputs.lock().await;
        registry.list().into_iter().find(|d| d.usn == req.device_id)
    };
    let Some(device) = device else {
        return (StatusCode::NOT_FOUND, "output device not found").into_response();
    };

    let lan_ip = match local_lan_ip() {
        Ok(ip) => ip,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let stream_url = format!(
        "http://{lan_ip}:{}/stream/audio.wav",
        crate::DEFAULT_PORT
    );

    let sender = SonosSender::new(device, reqwest::Client::new(), stream_url);
    match state.activate_sender(Box::new(sender)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

pub async fn set_output_volume(
    State(state): State<CoreState>,
    Json(req): Json<SetVolumeRequest>,
) -> Response {
    let mut guard = state.active_sender.lock().await;
    match guard.as_mut() {
        Some(sender) => match sender.set_volume(req.volume).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        None => (StatusCode::CONFLICT, "no active output").into_response(),
    }
}
