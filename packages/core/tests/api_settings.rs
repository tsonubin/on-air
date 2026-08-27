use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn eq_defaults_to_zero_and_can_be_updated() {
    let app = on_air_core::build_router(CoreState::new());

    let get_response = app
        .clone()
        .oneshot(Request::builder().uri("/api/eq").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["gains_db"], serde_json::json!([0.0, 0.0, 0.0, 0.0, 0.0]));

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/eq")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"gains_db":[3.0,0.0,0.0,0.0,-3.0]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response_2 = app
        .oneshot(Request::builder().uri("/api/eq").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(get_response_2.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["gains_db"], serde_json::json!([3.0, 0.0, 0.0, 0.0, -3.0]));
}

#[tokio::test]
async fn sample_rate_defaults_to_44100_and_can_be_updated() {
    let app = on_air_core::build_router(CoreState::new());

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sample-rate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["sample_rate_hz"], 44100);

    let put_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/sample-rate")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sample_rate_hz":48000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);
}
