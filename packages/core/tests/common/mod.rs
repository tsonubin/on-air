//! Shared by the `api_*` integration tests: a mock core behind its router
//! and the request/response helpers that drive it in-process.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

/// The router over a fresh mock core, plus the state behind it.
pub async fn mock_app() -> (axum::Router, CoreState) {
    let state = CoreState::new_mock().await;
    (on_air_core::build_router(state.clone()), state)
}

/// Sends one request and decodes the body as JSON (`Null` when empty or not JSON).
pub async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null)
    };
    (status, value)
}

pub fn json(method: &str, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub fn post_json(uri: &str, body: &str) -> Request<Body> {
    json("POST", uri, body)
}

pub fn put_json(uri: &str, body: &str) -> Request<Body> {
    json("PUT", uri, body)
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}
