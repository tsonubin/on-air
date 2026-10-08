use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::auth::authorize;
use on_air_core::pairing::{PairingState, VerifyError, MOCK_PIN};
use on_air_core::state::CoreState;
use std::net::SocketAddr;
use tower::ServiceExt;

#[test]
fn paired_tokens_are_bounded_and_oldest_is_evicted() {
    let mut pairing = on_air_core::pairing::PairingState::mock();
    let first = pairing.verify(on_air_core::pairing::MOCK_PIN).unwrap();
    for _ in 0..16 {
        pairing.verify(on_air_core::pairing::MOCK_PIN).unwrap();
    }
    assert!(!pairing.token_valid(&first));
}

#[test]
fn issued_token_is_required_for_control_paths() {
    let mut pairing = PairingState::mock();
    assert!(!authorize("/api/outputs", None, false, &pairing));
    let token = pairing.verify(MOCK_PIN).unwrap();
    assert!(authorize(
        "/api/outputs",
        Some(&format!("Bearer {token}")),
        false,
        &pairing
    ));
    assert!(authorize("/api/status", None, false, &pairing));
    assert!(authorize(
        "/stream/0123456789abcdef0123456789abcdef/audio.wav",
        None,
        false,
        &pairing
    ));
}

#[test]
fn production_pin_rotates_and_tokens_use_random_material() {
    let mut pairing = PairingState::new();
    let first_pin = pairing.pin().to_string();
    let token = pairing.verify(&first_pin).unwrap();
    assert_ne!(pairing.pin(), first_pin);
    assert_eq!(token.len(), "onair-".len() + 64);
    assert!(pairing.token_valid(&token));
}

#[test]
fn repeated_bad_pins_are_rate_limited() {
    let mut pairing = PairingState::mock();
    for _ in 0..5 {
        assert_eq!(pairing.verify("bad"), Err(VerifyError::InvalidPin));
    }
    assert_eq!(pairing.verify(MOCK_PIN), Err(VerifyError::RateLimited));
}

#[test]
fn production_remote_clients_need_a_token_loopback_does_not() {
    let pairing = PairingState::mock();
    assert!(
        authorize("/api/outputs", None, true, &pairing),
        "loopback desktop UI must work without pairing"
    );
    assert!(
        !authorize("/api/outputs", None, false, &pairing),
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
    let body = axum::body::to_bytes(bad.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "invalid_pin");

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
async fn pairing_pin_is_for_loopback_desktop_only() {
    let app = on_air_core::build_router(CoreState::new_mock().await);
    let remote = SocketAddr::from(([192, 168, 1, 20], 50000));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/pairing/pin")
                .extension(axum::extract::ConnectInfo(remote))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "forbidden");
}

/// Audit finding #5: without `ConnectInfo` the extractor used to assume
/// loopback. A production core must fail closed; only the mock core (used
/// by in-process tests) may treat a missing peer address as the desktop.
#[tokio::test]
async fn production_core_rejects_requests_without_token_or_peer_address() {
    let app = on_air_core::build_router(CoreState::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/outputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "not_paired");
}

#[tokio::test]
async fn pin_lockout_is_per_peer_over_http() {
    let app = on_air_core::build_router(CoreState::new_mock().await);
    let attacker = SocketAddr::from(([192, 168, 1, 66], 40000));
    let phone = SocketAddr::from(([192, 168, 1, 20], 40001));
    let verify = |peer: SocketAddr, pin: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/pairing/verify")
            .header("content-type", "application/json")
            .extension(axum::extract::ConnectInfo(peer))
            .body(Body::from(format!(r#"{{"pin":"{pin}"}}"#)))
            .unwrap()
    };
    for _ in 0..5 {
        let bad = app
            .clone()
            .oneshot(verify(attacker, "000000"))
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    }
    let locked = app
        .clone()
        .oneshot(verify(attacker, MOCK_PIN))
        .await
        .unwrap();
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = axum::body::to_bytes(locked.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "pin_lockout");

    let allowed = app.oneshot(verify(phone, MOCK_PIN)).await.unwrap();
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "another peer must not inherit the lockout"
    );
}

#[tokio::test]
async fn untrusted_browser_origin_cannot_use_loopback_exemption() {
    let app = on_air_core::build_router(CoreState::new_mock().await);
    let loopback = SocketAddr::from(([127, 0, 0, 1], 50000));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/outputs")
                .header("origin", "https://attacker.example")
                .extension(axum::extract::ConnectInfo(loopback))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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

#[test]
fn paired_phone_survives_desktop_restart_without_saving_bearer_token() {
    let dir = std::env::temp_dir().join(format!(
        "onair-pairing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = dir.join("settings.json");
    let state = on_air_core::state::CoreState::new_persistent(&path);
    let token = {
        let mut pairing = state.pairing.lock();
        let pin = pairing.pin().to_string();
        pairing.verify(&pin).unwrap()
    };
    drop(state);
    let restarted = on_air_core::state::CoreState::new_persistent(&path);
    assert!(restarted.pairing.lock().token_valid(&token));
    let persisted = std::fs::read_to_string(dir.join("paired-remotes.json")).unwrap();
    assert!(!persisted.contains(&token));
    drop(restarted);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pairing_reports_storage_failure_instead_of_promising_a_saved_pairing() {
    let path = std::env::temp_dir().join(format!("onair-blocked-{}", std::process::id()));
    std::fs::write(&path, b"not a directory").unwrap();
    let mut pairing = PairingState::persistent(path.join("paired-remotes.json"));
    let pin = pairing.pin().to_string();
    assert_eq!(pairing.verify(&pin), Err(VerifyError::StorageUnavailable));
    std::fs::remove_file(path).unwrap();
}
