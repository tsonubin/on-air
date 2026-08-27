use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn list_inputs_returns_ok_json_array() {
    let app = on_air_core::build_router(CoreState::new());

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
    assert!(json["inputs"].is_array());
}

#[tokio::test]
async fn activate_unknown_input_returns_404() {
    let app = on_air_core::build_router(CoreState::new());

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
}
