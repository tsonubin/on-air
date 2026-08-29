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

#[tokio::test]
async fn sample_rate_lists_input_and_output_supported_rates() {
    let app = on_air_core::build_router(CoreState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sample-rate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["input"]["sample_rate_hz"], 44100);
    assert_eq!(json["output"]["sample_rate_hz"], 44100);
    let input_rates = json["input"]["supported_hz"].as_array().unwrap();
    assert!(input_rates.iter().any(|v| v == 44100));
    assert!(input_rates.iter().any(|v| v == 48000));
    let output_rates = json["output"]["supported_hz"].as_array().unwrap();
    assert!(output_rates.iter().any(|v| v == 44100));
    assert!(output_rates.iter().any(|v| v == 48000));
}

#[tokio::test]
async fn sample_rate_rejects_rates_outside_supported_selection() {
    let app = on_air_core::build_router(CoreState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/sample-rate")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sample_rate_hz":22050}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sample_rate_can_set_output_independently() {
    let app = on_air_core::build_router(CoreState::new());
    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/sample-rate")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"output_hz":48000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
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
    assert_eq!(json["input"]["sample_rate_hz"], 44100);
    assert_eq!(json["output"]["sample_rate_hz"], 48000);
}
