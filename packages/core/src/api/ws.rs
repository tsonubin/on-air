use crate::auth::Paired;
use crate::state::CoreState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use serde::Serialize;
use std::time::Duration;

/// Keep-alive interval; clients treat a 20 s silence as a dead socket.
pub const PING_INTERVAL: Duration = Duration::from_secs(10);

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
    ServiceStateChanged {
        enabled: bool,
    },
    CdStateChanged {
        present: bool,
        playing: bool,
        track: u8,
        track_count: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        album: Option<String>,
        position_ms: u64,
        duration_ms: u64,
    },
}

pub async fn ws_handler(
    Paired: Paired,
    ws: WebSocketUpgrade,
    State(state): State<CoreState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: CoreState) {
    let mut rx = state.ws_tx.subscribe();
    let mut ping =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                let Ok(text) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    None | Some(Err(_)) => break,
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}
