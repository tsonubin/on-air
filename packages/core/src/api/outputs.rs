use crate::sender::airplay::AirPlaySender;
use crate::sender::bluetooth::{BluetoothSender, CpalPcmSink, PcmSink};
use crate::sender::sonos::{net::local_lan_ip, SonosSender};
use crate::sender::{AudioSender, NullSender};
use crate::auth::Paired;
use crate::state::{ActiveOutput, CoreState};
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

pub async fn list_outputs(Paired: Paired, State(state): State<CoreState>) -> Json<OutputsResponse> {
    let mut outputs = Vec::new();
    for d in state.outputs.lock().await.list() {
        outputs.push(OutputInfo {
            id: d.usn,
            name: d.friendly_name,
            transport: "sonos",
        });
    }
    for d in state.airplay_outputs.lock().unwrap().iter() {
        outputs.push(OutputInfo {
            id: d.id.clone(),
            name: d.name.clone(),
            transport: "airplay",
        });
    }
    for d in state.bluetooth.list() {
        outputs.push(OutputInfo {
            id: d.id,
            name: d.name,
            transport: "bluetooth",
        });
    }
    Json(OutputsResponse { outputs })
}

pub async fn get_active_output(Paired: Paired, State(state): State<CoreState>) -> Json<Option<ActiveOutput>> {
    Json(state.active_output.lock().unwrap().clone())
}

#[derive(Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

pub async fn activate_output(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<ActivateOutputRequest>,
) -> Response {
    let sender: Box<dyn AudioSender> = match req.transport.as_str() {
        "sonos" => {
            let device = {
                let registry = state.outputs.lock().await;
                registry.list().into_iter().find(|d| d.usn == req.device_id)
            };
            let Some(device) = device else {
                return (StatusCode::NOT_FOUND, "output device not found").into_response();
            };
            let name = device.friendly_name.clone();
            *state.active_output.lock().unwrap() = Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: req.device_id.clone(),
                device_name: name.clone(),
            });
            if state.mock {
                Box::new(NullSender::new(name, state.mock_log.clone()))
            } else {
                let lan_ip = match crate::sender::sonos::net::local_lan_ip_toward(device.ip) {
                    Ok(ip) => ip,
                    Err(_) => match local_lan_ip() {
                        Ok(ip) => ip,
                        Err(e) => {
                            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                        }
                    },
                };
                let stream_url = format!("http://{lan_ip}:{}/stream/audio.wav", crate::DEFAULT_PORT);
                Box::new(SonosSender::new(
                    device,
                    crate::sender::sonos::soap::http_client(),
                    stream_url,
                ))
            }
        }
        "airplay" => {
            if crate::sender::airplay::platform_mode() == "avroute-picker" && !state.mock {
                return (
                    StatusCode::BAD_REQUEST,
                    "macOS AirPlay is local-picker only",
                )
                    .into_response();
            }
            let device = state
                .airplay_outputs
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == req.device_id)
                .cloned();
            let Some(device) = device else {
                return (StatusCode::NOT_FOUND, "output device not found").into_response();
            };
            *state.active_output.lock().unwrap() = Some(ActiveOutput {
                transport: "airplay".into(),
                device_id: device.id.clone(),
                device_name: device.name.clone(),
            });
            if state.mock {
                Box::new(NullSender::new(device.name, state.mock_log.clone()))
            } else {
                let base = state.owntone_base.lock().unwrap().clone();
                Box::new(AirPlaySender::new(device.name, device.id, base))
            }
        }
        "bluetooth" => {
            let device = state
                .bluetooth
                .list()
                .into_iter()
                .find(|d| d.id == req.device_id);
            let Some(device) = device else {
                return (StatusCode::NOT_FOUND, "output device not found").into_response();
            };
            *state.active_output.lock().unwrap() = Some(ActiveOutput {
                transport: "bluetooth".into(),
                device_id: device.id.clone(),
                device_name: device.name.clone(),
            });
            let sink: std::sync::Arc<dyn PcmSink> = if state.mock {
                state.pcm_sink.clone()
            } else {
                std::sync::Arc::new(CpalPcmSink::for_output_named(&device.name))
            };
            Box::new(BluetoothSender::new(
                device.id,
                device.name,
                state.bluetooth.clone(),
                state.audio_tx.clone(),
                sink,
            ))
        }
        _ => {
            return (StatusCode::BAD_REQUEST, "unknown transport").into_response();
        }
    };

    match state.activate_sender(sender).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

pub async fn set_output_volume(
    Paired: Paired,
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
