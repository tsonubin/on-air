//! `PUT /api/service`: turn the LAN audio service on or off.
//!
//! The same transition the desktop tray performs: persist the flag, tell
//! WebSocket clients, hold or release the sleep inhibitor, then stop the
//! live audio work (off) or restore the saved session (on). Authorized
//! rather than Paired: turning the service back on must work while paused.

use crate::api::auth::Authorized;
use crate::api::error::JsonBody;
use crate::state::CoreState;
use axum::extract::State;
use axum::Json;

pub use on_air_api::{ServiceResponse, SetServiceRequest};

pub async fn set_service(
    Authorized: Authorized,
    State(state): State<CoreState>,
    JsonBody(body): JsonBody<SetServiceRequest>,
) -> Json<ServiceResponse> {
    let transition = state.begin_service_transition(body.enabled);
    // Detached so a client that disconnects (the TUI quitting right after
    // `s`, a killed CLI) cannot cancel the shutdown halfway, the same
    // guarantee `run_detached` gives output switches.
    let finishing = state.clone();
    let _ =
        tokio::spawn(async move { finishing.finish_service_transition(transition).await }).await;
    Json(ServiceResponse {
        enabled: state
            .service_enabled
            .load(std::sync::atomic::Ordering::Acquire),
    })
}
