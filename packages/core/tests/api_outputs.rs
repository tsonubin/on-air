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
                .body(Body::from(
                    r#"{"transport":"sonos","device_id":"does-not-exist"}"#,
                ))
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

#[tokio::test]
async fn volume_above_one_hundred_is_rejected() {
    let app = on_air_core::build_router(CoreState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active/volume")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"volume":101}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn volume_can_be_read_back_after_a_successful_change() {
    let state = CoreState::new_mock().await;
    let app = on_air_core::build_router(state);

    let activate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::NO_CONTENT);

    let set = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active/volume")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"volume":37}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(set.status(), StatusCode::NO_CONTENT);

    let get = app
        .oneshot(
            Request::builder()
                .uri("/api/outputs/active/volume")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["volume"], 37);
}
