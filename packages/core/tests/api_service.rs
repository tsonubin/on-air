use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::events::WsEvent;
use on_air_core::keep_awake::testing::{fake_method, Event};
use on_air_core::state::CoreState;
use std::sync::atomic::Ordering;
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

fn put_json(uri: &str, body: &str) -> Request<Body> {
    json("PUT", uri, body)
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    json("POST", uri, body)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn turning_the_service_off_and_on_round_trips_and_broadcasts() {
    let (app, state) = mock_app().await;
    let mut events = state.ws_tx.subscribe();

    let (status, body) = send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    assert!(!state.service_enabled.load(Ordering::Acquire));
    assert!(matches!(
        events.try_recv(),
        Ok(WsEvent::ServiceStateChanged { enabled: false })
    ));
    let (_, status_body) = send(app.clone(), get("/api/status")).await;
    assert_eq!(status_body["service_enabled"], false);

    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":true}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert!(matches!(
        events.try_recv(),
        Ok(WsEvent::ServiceStateChanged { enabled: true })
    ));
}

#[tokio::test]
async fn a_repeated_request_is_idempotent_and_emits_nothing() {
    let (app, state) = mock_app().await;
    let mut events = state.ws_tx.subscribe();
    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":true}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn a_bad_body_is_a_400_envelope() {
    let (app, _) = mock_app().await;
    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":"yes"}"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn turning_off_stops_the_live_output() {
    let (app, state) = mock_app().await;
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert!(status.is_success(), "activation: {status}");
    assert!(state.active_output().is_some());
    send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    )
    .await;
    assert!(state.active_output().is_none());
}

#[tokio::test]
async fn concurrent_toggles_leave_flag_and_inhibitor_in_step() {
    let (keep_awake, spawner) = on_air_core::keep_awake::testing::recording(true);
    let state = CoreState::new_mock().await.with_keep_awake(keep_awake);
    let app = on_air_core::build_router(state.clone());
    let off = send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    );
    let off_again = send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    );
    tokio::join!(off, off_again);
    assert!(!state.service_enabled.load(Ordering::Acquire));
    assert_eq!(
        spawner.events(),
        vec![Event::Started(fake_method()), Event::Released],
        "one release, not two"
    );
    assert!(!state.sleep_inhibited());
}
