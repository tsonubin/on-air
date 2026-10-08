use crate::api::error::ApiError;
use crate::pairing::PairingState;
use crate::state::CoreState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::atomic::Ordering;

fn bearer_token(authorization: Option<&str>) -> Option<&str> {
    authorization.and_then(|h| h.strip_prefix("Bearer "))
}

fn query_token(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == "token" && !v.is_empty() {
            return Some(
                v.replace("%2F", "/")
                    .replace("%2B", "+")
                    .replace("%3D", "="),
            );
        }
    }
    None
}

/// Routes a speaker or an unpaired phone must reach: status, the PIN
/// handshake and the radio stream (`/stream/<nonce>/audio.wav`).
pub fn is_public_path(path: &str) -> bool {
    matches!(path, "/api/status" | "/api/pairing/verify") || path.starts_with("/stream/")
}

/// Desktop loopback is allowed without a token. Remote clients must present a
/// Bearer token issued by `/api/pairing/verify`.
pub fn authorize(
    path: &str,
    authorization: Option<&str>,
    peer_is_loopback: bool,
    pairing: &PairingState,
) -> bool {
    if is_public_path(path) {
        return true;
    }
    if let Some(token) = bearer_token(authorization) {
        if pairing.token_valid(token) {
            return true;
        }
    }
    peer_is_loopback
}

pub struct Paired;

fn trusted_desktop_origin(origin: Option<&str>) -> bool {
    origin.is_none_or(|origin| {
        matches!(
            origin,
            "http://127.0.0.1:1420"
                | "http://localhost:1420"
                | "tauri://localhost"
                | "http://tauri.localhost"
                | "https://tauri.localhost"
        )
    })
}

/// Whether the TCP peer is loopback. `allow_missing` decides what to do when
/// the server was not built with `ConnectInfo` (only the mock core, used by
/// in-process tests, is allowed to treat that as loopback).
fn peer_is_loopback(parts: &Parts, allow_missing: bool) -> bool {
    parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip().is_loopback())
        .unwrap_or(allow_missing)
}

#[async_trait::async_trait]
impl FromRequestParts<CoreState> for Paired {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CoreState,
    ) -> Result<Self, Self::Rejection> {
        if !state.service_enabled.load(Ordering::Acquire) {
            return Err(ApiError::service_paused());
        }
        let path = parts.uri.path();
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let query_bearer = query_token(parts.uri.query()).map(|t| format!("Bearer {t}"));
        let authorization = header.or(query_bearer.as_deref());
        let origin = parts
            .headers
            .get(axum::http::header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let connect_loopback =
            peer_is_loopback(parts, state.mock) && trusted_desktop_origin(origin);
        let peer_is_loopback = if state.require_auth {
            false
        } else {
            connect_loopback
        };
        let pairing = state.pairing.lock();
        if authorize(path, authorization, peer_is_loopback, &pairing) {
            Ok(Paired)
        } else {
            Err(ApiError::not_paired())
        }
    }
}

/// Endpoints such as the desktop-displayed pairing PIN must never be readable
/// from another LAN host, even if that host already has a control token.
pub struct LocalClient;

#[async_trait::async_trait]
impl FromRequestParts<CoreState> for LocalClient {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &CoreState,
    ) -> Result<Self, Self::Rejection> {
        if !state.service_enabled.load(Ordering::Acquire) {
            return Err(ApiError::service_paused());
        }
        let origin = parts
            .headers
            .get(axum::http::header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if peer_is_loopback(parts, state.mock) && trusted_desktop_origin(origin) {
            Ok(LocalClient)
        } else {
            Err(ApiError::forbidden(
                "this endpoint is only available to the desktop app",
            ))
        }
    }
}
