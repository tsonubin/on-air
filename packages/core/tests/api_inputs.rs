use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use std::net::SocketAddr;
use tower::ServiceExt;

#[tokio::test]
async fn list_inputs_returns_ok_json_array() {
    let app = on_air_core::build_router(CoreState::new_mock().await);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["inputs"], serde_json::json!(["Mock Monitor"]));
}

#[tokio::test]
async fn activate_unknown_input_returns_404_envelope() {
    let app = on_air_core::build_router(CoreState::new_mock().await);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inputs/active")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"definitely-not-a-real-device"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "not_found");
    assert_eq!(json["error"], "input device not found");
}

/// A production (non-mock) core still serves the loopback desktop without a
/// token, as long as the server attached `ConnectInfo`.
#[tokio::test]
async fn production_core_serves_loopback_peer_with_connect_info() {
    let app = on_air_core::build_router(CoreState::new());
    let loopback = SocketAddr::from(([127, 0, 0, 1], 50000));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inputs/active")
                .extension(axum::extract::ConnectInfo(loopback))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], serde_json::Value::Null);
    assert!(json["backend"].is_string());
}
