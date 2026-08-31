use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::auth::authorize;
use on_air_core::pairing::{PairingState, MOCK_PIN};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[test]
fn issued_token_is_required_for_control_paths() {
    let mut pairing = PairingState::mock();
    assert!(!authorize("/api/outputs", None, false, &pairing, true));
    let token = pairing.verify(MOCK_PIN).unwrap();
    assert!(authorize(
        "/api/outputs",
        Some(&format!("Bearer {token}")),
        false,
        &pairing,
        true
    ));
    assert!(authorize("/api/status", None, false, &pairing, true));
}

#[test]
fn production_remote_clients_need_a_token_loopback_does_not() {
    let pairing = PairingState::mock();
    assert!(
        authorize("/api/outputs", None, true, &pairing, false),
        "loopback desktop UI must work without pairing"
    );
    assert!(
        !authorize("/api/outputs", None, false, &pairing, false),
        "remote clients must pair before control routes"
    );
}

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

#[tokio::test]
async fn control_routes_require_token_when_require_auth_is_set() {
    let mut state = CoreState::new_mock().await;
    state.require_auth = true;
    let app = on_air_core::build_router(state);

    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/outputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let verify = app
        .clone()
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
    let body = axum::body::to_bytes(verify.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = json["token"].as_str().unwrap();

    let allowed = app
        .oneshot(
            Request::builder()
                .uri("/api/outputs")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn websocket_style_query_token_authorizes_remote_clients() {
    let mut state = CoreState::new_mock().await;
    state.require_auth = true;
    let app = on_air_core::build_router(state);

    let verify = app
        .clone()
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
    let body = axum::body::to_bytes(verify.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let token = json["token"].as_str().unwrap();

    let allowed = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/outputs?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}
