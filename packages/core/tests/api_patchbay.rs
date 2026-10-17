use axum::http::StatusCode;
use serde_json::{json, Value};
mod common;
use common::*;

async fn command(app: axum::Router, body: Value) -> (StatusCode, Value) {
    send(app, json("POST", "/api/patchbay", &body.to_string())).await
}

#[tokio::test]
async fn groups_fan_out_to_independent_network_sessions_and_survive_the_client() {
    let (app, state) = mock_app().await;
    let (status, snapshot) = send(app.clone(), get("/api/patchbay")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["supported"], true);
    let network: Vec<_> = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == "sonos" || n["kind"] == "airplay")
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(network.len(), 2);
    let (status, snapshot) = command(
        app.clone(),
        json!({"op":"combine","side":"output","members":network}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    let group = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "group")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let (status, snapshot) = command(
        app.clone(),
        json!({"op":"connect","source":"desktop","target":group}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    assert_eq!(snapshot["links"][0]["state"], "connected", "{snapshot}");
    assert_eq!(
        snapshot["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["state"] == "playing")
            .count(),
        2
    );
    assert!(
        state.active_output().is_none(),
        "patchbay sessions must not steal the legacy session"
    );
    drop(app);
    let calls = state.mock_log.lock().await.clone();
    assert_eq!(
        calls.iter().filter(|call| call.ends_with(":start")).count(),
        2,
        "{calls:?}"
    );
    state.shutdown().await;
    let calls = state.mock_log.lock().await.clone();
    assert_eq!(
        calls.iter().filter(|call| call.ends_with(":stop")).count(),
        2,
        "{calls:?}"
    );
}

#[tokio::test]
async fn defaults_volume_mute_and_disconnect_are_real_core_state() {
    let (app, state) = mock_app().await;
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"set_default","id":"desktop-output"}),
    )
    .await;
    assert!(snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == "desktop-output" && n["is_default"] == true));
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"set_volume","id":"sink:headphones","volume":37}),
    )
    .await;
    assert_eq!(
        snapshot["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "sink:headphones")
            .unwrap()["volume"],
        37
    );
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"set_muted","id":"sink:headphones","muted":true}),
    )
    .await;
    assert_eq!(
        snapshot["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "sink:headphones")
            .unwrap()["muted"],
        true
    );
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"connect","source":"source:mic","target":"sink:headphones"}),
    )
    .await;
    assert_eq!(snapshot["links"].as_array().unwrap().len(), 1);
    let (_, snapshot) = command(
        app,
        json!({"op":"disconnect","source":"source:mic","target":"sink:headphones"}),
    )
    .await;
    assert!(snapshot["links"].as_array().unwrap().is_empty());
    state.shutdown().await;
}

#[tokio::test]
async fn feedback_and_wrong_side_groups_are_rejected_without_changing_routes() {
    let (app, state) = mock_app().await;
    let (status, error) = command(
        app.clone(),
        json!({"op":"connect","source":"desktop","target":"desktop-output"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("feedback"));
    let (status, _) = command(
        app.clone(),
        json!({"op":"combine","side":"input","members":["source:mic","sink:headphones"]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, snapshot) = send(app, get("/api/patchbay")).await;
    assert!(snapshot["links"].as_array().unwrap().is_empty());
    assert!(!snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"] == "group"));
    state.shutdown().await;
}

#[tokio::test]
async fn input_mix_membership_can_expand_and_removal_disconnects_its_routes() {
    let (app, state) = mock_app().await;
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"combine","side":"input","members":["source:mic","desktop"]}),
    )
    .await;
    let group = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "group")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, snapshot) = command(
        app.clone(),
        json!({"op":"connect","source":group,"target":"sink:headphones"}),
    )
    .await;
    assert_eq!(snapshot["links"][0]["state"], "connected");
    let (_, snapshot) = command(app, json!({"op":"remove_group","id":group})).await;
    assert!(snapshot["links"].as_array().unwrap().is_empty());
    assert!(!snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["kind"] == "group"));
    state.shutdown().await;
}

#[tokio::test]
async fn paused_service_allows_inspection_but_rejects_audio_changes() {
    let (app, state) = mock_app().await;
    send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    )
    .await;
    let (status, snapshot) = send(app.clone(), get("/api/patchbay")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["enabled"], false);
    let (status, error) = command(app, json!({"op":"set_default","id":"sink:headphones"})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["code"], "service_paused");
    state.shutdown().await;
}

#[tokio::test]
async fn legacy_clients_cannot_steal_a_patchbay_receiver() {
    let (app, state) = mock_app().await;
    let (_, snapshot) = send(app.clone(), get("/api/patchbay")).await;
    let receiver = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "sonos")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let (status, _) = command(
        app.clone(),
        json!({"op":"connect","source":"desktop","target":receiver}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, error) = send(
        app,
        post_json(
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "not_ready");
    assert!(state.active_output().is_none());
    state.shutdown().await;
}
