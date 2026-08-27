use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn activate_unknown_output_returns_404() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"transport":"sonos","device_id":"does-not-exist"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn volume_with_no_active_output_returns_409() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active/volume")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"volume":50}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
