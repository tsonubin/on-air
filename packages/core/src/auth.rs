use crate::pairing::PairingState;
use crate::state::CoreState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;

pub fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/api/status" | "/api/pairing/pin" | "/api/pairing/verify" | "/stream/audio.wav"
    )
}

/// Desktop loopback is allowed without a token. Remote clients must present a
/// Bearer token issued by `/api/pairing/verify`.
pub fn authorize(
    path: &str,
    authorization: Option<&str>,
    peer_is_loopback: bool,
    pairing: &PairingState,
    require_token: bool,
) -> bool {
    if is_public_path(path) {
        return true;
    }
    if let Some(header) = authorization {
        if let Some(token) = header.strip_prefix("Bearer ") {
            if pairing.token_valid(token) {
                return true;
            }
        }
    }
    if require_token {
        return false;
    }
    peer_is_loopback
}

pub struct Paired;

#[async_trait::async_trait]
impl FromRequestParts<CoreState> for Paired {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CoreState,
    ) -> Result<Self, Self::Rejection> {
        let path = parts.uri.path();
        let authorization = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let connect_loopback = parts
            .extensions
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip().is_loopback())
            .unwrap_or(true);
        let peer_is_loopback = if state.require_auth {
            false
        } else {
            connect_loopback
        };
        let pairing = state.pairing.lock().unwrap();
        if authorize(path, authorization, peer_is_loopback, &pairing, false) {
            Ok(Paired)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
