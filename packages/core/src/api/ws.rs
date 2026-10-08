use crate::api::auth::Paired;
use crate::state::CoreState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use std::time::Duration;

/// Keep-alive interval for the `{"type":"Heartbeat"}` text frame; clients
/// treat a long silence as a dead socket. A text frame (not a protocol ping)
/// because browsers and React Native never surface pings to the app.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

pub use crate::events::WsEvent;

pub async fn ws_handler(
    Paired: Paired,
    ws: WebSocketUpgrade,
    State(state): State<CoreState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: CoreState) {
    let mut rx = state.ws_tx.subscribe();
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                if send_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
            _ = heartbeat.tick() => {
                if send_event(&mut socket, &WsEvent::Heartbeat).await.is_err() {
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

async fn send_event(socket: &mut WebSocket, event: &WsEvent) -> Result<(), ()> {
    let Ok(text) = serde_json::to_string(event) else {
        return Ok(());
    };
    socket.send(Message::Text(text)).await.map_err(|_| ())
}
