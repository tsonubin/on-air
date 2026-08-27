use crate::state::CoreState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    OutputStateChanged {
        transport: String,
        device_name: String,
        active: bool,
    },
    LevelMeter {
        rms: f32,
        peak: f32,
    },
    DeviceJoined {
        transport: String,
        id: String,
        name: String,
    },
    DeviceLeft {
        transport: String,
        id: String,
    },
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<CoreState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: CoreState) {
    let mut rx = state.ws_tx.subscribe();
    while let Ok(event) = rx.recv().await {
        let Ok(text) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(text)).await.is_err() {
            break;
        }
    }
}
