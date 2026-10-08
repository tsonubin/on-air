use crate::api::error::{ApiError, JsonBody};
use crate::auth::LocalClient;
use crate::pairing::persist_tokens;
use crate::state::CoreState;
use axum::extract::{ConnectInfo, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;

#[derive(Serialize)]
pub struct PinResponse {
    pub pin: String,
}

pub async fn get_pin(
    LocalClient: LocalClient,
    State(state): State<CoreState>,
) -> Json<PinResponse> {
    Json(PinResponse {
        pin: state.pairing.lock().pin().to_string(),
    })
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub pin: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub token: String,
}

/// PIN → bearer token. The PIN check and the commit each take the `pairing`
/// mutex briefly; the on-disk write runs on a blocking thread in between so
/// request extractors are never stalled behind an fsync.
pub async fn verify_pin(
    State(state): State<CoreState>,
    peer: Option<ConnectInfo<SocketAddr>>,
    JsonBody(req): JsonBody<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiError> {
    if !state.service_enabled.load(Ordering::Acquire) {
        return Err(ApiError::service_paused());
    }
    let peer_ip = peer.map(|ConnectInfo(addr)| addr.ip());
    let _serialised = state.pairing_verify_lock.lock().await;
    let prepared = state.pairing.lock().begin_verify(&req.pin, peer_ip)?;
    if let Some(path) = prepared.storage_path.clone() {
        let tokens = prepared.tokens.clone();
        let written = tokio::task::spawn_blocking(move || persist_tokens(&path, &tokens))
            .await
            .map_err(|error| ApiError::internal(format!("pairing write task failed: {error}")))?;
        if let Err(error) = written {
            eprintln!("Could not save remote pairing: {error}");
            return Err(ApiError::internal(
                "could not save the pairing on the desktop",
            ));
        }
    }
    let token = state.pairing.lock().commit(prepared);
    Ok(Json(VerifyResponse { token }))
}
