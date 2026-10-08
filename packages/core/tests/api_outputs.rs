use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

async fn mock_app() -> (axum::Router, CoreState) {
    let state = CoreState::new_mock().await;
    (on_air_core::build_router(state.clone()), state)
}

async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
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

fn json(method: &str, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn activate_unknown_output_returns_404_envelope() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        json(
            "POST",
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"does-not-exist"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn unknown_transport_is_a_validation_error() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        json(
            "POST",
            "/api/outputs/active",
            r#"{"transport":"carrier-pigeon","device_id":"x"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn malformed_json_is_a_400_envelope() {
    let (app, _) = mock_app().await;
    let (status, body) = send(app, json("POST", "/api/outputs/active", r#"{"transport":"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn volume_with_no_active_output_returns_404() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        json("POST", "/api/outputs/active/volume", r#"{"volume":50}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "no_active_output");
}

#[tokio::test]
async fn volume_above_one_hundred_is_rejected() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        json("POST", "/api/outputs/active/volume", r#"{"volume":101}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn active_output_is_wrapped_and_delete_stops_casting() {
    let (app, state) = mock_app().await;

    let (status, body) = send(app.clone(), get("/api/outputs/active")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({ "active": null }));

    let (status, _) = send(
        app.clone(),
        json(
            "POST",
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(app.clone(), get("/api/outputs/active")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"]["transport"], "sonos");
    assert_eq!(body["active"]["device_id"], "uuid:mock-sonos");
    assert_eq!(body["active"]["device_name"], "Mock Sonos");
    assert_eq!(body["active"]["state"], "live");

    let (status, _) = send(
        app.clone(),
        Request::builder()
            .method("DELETE")
            .uri("/api/outputs/active")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(state.active_output().is_none());
    assert_eq!(
        state.mock_log.lock().await.as_slice(),
        [
            "Mock Sonos:start",
            "Mock Sonos:volume:50",
            "Mock Sonos:stop"
        ]
    );

    let (status, body) = send(app, get("/api/outputs/active")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], serde_json::Value::Null);
}

#[tokio::test]
async fn volume_can_be_read_back_after_a_successful_change() {
    let (app, _) = mock_app().await;

    let (status, _) = send(
        app.clone(),
        json(
            "POST",
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send(
        app.clone(),
        json("POST", "/api/outputs/active/volume", r#"{"volume":37}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(app, get("/api/outputs/active/volume")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["volume"], 37);
}

#[tokio::test]
async fn unknown_routes_get_the_envelope_too() {
    let (app, _) = mock_app().await;
    let (status, body) = send(app, get("/api/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}
