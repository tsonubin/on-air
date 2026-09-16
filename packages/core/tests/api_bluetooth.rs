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

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn outputs_mark_unpaired_bluetooth_as_needing_pair() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        Request::builder()
            .uri("/api/outputs")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let outputs = body["outputs"].as_array().expect("outputs");
    let unpaired = outputs
        .iter()
        .find(|row| row["id"] == "bt-unpaired")
        .expect("unpaired bluetooth row");
    assert_eq!(unpaired["transport"], "bluetooth");
    assert_eq!(unpaired["needs_pair"], true);
    assert_eq!(unpaired["paired"], false);
    let paired = outputs
        .iter()
        .find(|row| row["id"] == "bt-speaker")
        .expect("paired bluetooth row");
    assert_eq!(paired["needs_pair"], false);
    assert_eq!(paired["paired"], true);
}

#[tokio::test]
async fn pairing_an_unknown_bluetooth_device_is_not_found() {
    let (app, _) = mock_app().await;
    let (status, _) = send(app, post_json("/api/bluetooth/pair", r#"{"id":"missing"}"#)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pairing_marks_the_device_paired() {
    let (app, _) = mock_app().await;
    let (status, _) = send(
        app.clone(),
        post_json("/api/bluetooth/pair", r#"{"id":"bt-unpaired"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(
        app,
        Request::builder()
            .uri("/api/bluetooth/devices")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let unpaired = body["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "bt-unpaired")
        .unwrap();
    assert_eq!(unpaired["paired"], true);
}

#[tokio::test]
async fn bluetooth_settings_opens_without_a_body() {
    let (app, _) = mock_app().await;
    let (status, _) = send(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/bluetooth/settings")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
