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
async fn empty_deck_is_not_present() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        Request::builder()
            .uri("/api/cd")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["present"], false);
    assert_eq!(body["playing"], false);
}

#[tokio::test]
async fn insert_autoplays_track_one_and_lists_cd_input() {
    let (app, state) = mock_app().await;
    let (status, body) = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"album":"Kind of Blue","tracks":[{"title":"So What"},{"title":"Freddie Freeloader"}]}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["present"], true);
    assert_eq!(body["playing"], true);
    assert_eq!(body["track"], 1);
    assert_eq!(body["track_count"], 2);
    assert_eq!(body["title"], "So What");
    assert_eq!(body["album"], "Kind of Blue");
    assert_eq!(state.input().active_name().as_deref(), Some("Audio CD"));

    let (status, listed) = send(
        app,
        Request::builder()
            .uri("/api/inputs")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names = listed["inputs"].as_array().unwrap();
    assert_eq!(names[0], "Audio CD");
}

#[tokio::test]
async fn transport_controls_change_track_and_pause() {
    let (app, _) = mock_app().await;
    let _ = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A"},{"title":"B"},{"title":"C"}]}"#,
        ),
    )
    .await;

    let (status, body) = send(
        app.clone(),
        post_json("/api/cd/control", r#"{"action":"next"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["track"], 2);
    assert_eq!(body["title"], "B");

    let (status, body) = send(
        app.clone(),
        post_json("/api/cd/control", r#"{"action":"pause"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["playing"], false);

    let (status, body) = send(
        app.clone(),
        post_json("/api/cd/control", r#"{"action":"play"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["playing"], true);

    let (status, body) = send(app, post_json("/api/cd/control", r#"{"action":"prev"}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["track"], 1);
}

#[tokio::test]
async fn control_without_disc_conflicts() {
    let (app, _) = mock_app().await;
    let (status, body) = send(app, post_json("/api/cd/control", r#"{"action":"play"}"#)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "conflict");
}

#[tokio::test]
async fn unknown_cd_action_is_a_400_envelope() {
    let (app, _) = mock_app().await;
    let _ = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A"}]}"#,
        ),
    )
    .await;
    let (status, body) = send(
        app,
        post_json("/api/cd/control", r#"{"action":"rewind-tape"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn insert_lists_per_track_durations() {
    let (app, _) = mock_app().await;
    let (status, body) = send(
        app,
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A","duration_ms":90000},{"title":"B","duration_ms":120000}]}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tracks"][0]["duration_ms"], 90_000);
    assert_eq!(body["tracks"][1]["duration_ms"], 120_000);
}

#[tokio::test]
async fn seek_and_goto_update_status() {
    let (app, _) = mock_app().await;
    let _ = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A"},{"title":"B"}]}"#,
        ),
    )
    .await;
    let (status, body) = send(
        app.clone(),
        post_json("/api/cd/control", r#"{"action":"goto","track":2}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["track"], 2);
    let (status, body) = send(
        app,
        post_json("/api/cd/control", r#"{"action":"seek","position_ms":1500}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // CD positions have 75-sector/second precision: 1500 ms floors to sector 112.
    assert_eq!(body["position_ms"], 1493);
}

#[tokio::test]
async fn software_eject_clears_the_deck() {
    let (app, state) = mock_app().await;
    let _ = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A"}]}"#,
        ),
    )
    .await;
    let (status, body) = send(app, post_json("/api/cd/control", r#"{"action":"eject"}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["present"], false);
    assert!(state.input().active_name().is_none());
}

#[tokio::test]
async fn eject_clears_cd_input() {
    let (app, state) = mock_app().await;
    let _ = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"tracks":[{"title":"A"}]}"#,
        ),
    )
    .await;
    let (status, body) = send(app, post_json("/api/mock/cd", r#"{"present":false}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["present"], false);
    assert!(state.input().active_name().is_none());
}

#[tokio::test]
async fn simulator_is_not_routed_on_a_live_core() {
    let app = on_air_core::build_router(CoreState::new());
    let loopback = std::net::SocketAddr::from(([127, 0, 0, 1], 50000));
    let (status, body) = send(
        app,
        Request::builder()
            .method("POST")
            .uri("/api/mock/cd")
            .header("content-type", "application/json")
            .extension(axum::extract::ConnectInfo(loopback))
            .body(Body::from(r#"{"present":true}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}
