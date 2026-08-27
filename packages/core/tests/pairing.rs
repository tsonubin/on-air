use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::pairing::MOCK_PIN;
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn pin_verify_issues_a_token_and_rejects_bad_pin() {
    let app = on_air_core::build_router(CoreState::new_mock().await);

    let pin_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/pairing/pin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pin_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(pin_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pin"], MOCK_PIN);

    let bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pairing/verify")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"pin":"000000"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);

    let ok = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pairing/verify")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"pin":"{MOCK_PIN}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let body = axum::body::to_bytes(ok.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["token"].as_str().unwrap().starts_with("onair-"));
}
