use axum::body::Body;
use axum::http::{Request, StatusCode};
mod common;
use common::*;

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

// ---------------------------------------------------------------------------
// Channel mode (roadmap A4)
// ---------------------------------------------------------------------------

/// A mono "This computer" output, as Pulse would report one.
fn mono_speaker() -> on_air_core::sender::local::LocalOutput {
    on_air_core::sender::local::LocalOutput {
        id: "alsa#mono".into(),
        name: "Mono Speaker".into(),
        sink: "alsa".into(),
        port: None,
        target: "alsa".into(),
        channels: Some(1),
    }
}

async fn listed_output(app: &axum::Router, id: &str) -> serde_json::Value {
    let (status, body) = send(app.clone(), get("/api/outputs")).await;
    assert_eq!(status, StatusCode::OK);
    body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap_or_else(|| panic!("output {id} not listed"))
        .clone()
}

#[tokio::test]
async fn stereo_on_a_mono_device_is_rejected_with_an_explanation() {
    let (app, state) = mock_app().await;
    state.set_mock_local_outputs(vec![mono_speaker()]);

    let (status, body) = send(
        app.clone(),
        json(
            "PUT",
            "/api/outputs/channel-mode",
            r#"{"transport":"local","device_id":"alsa#mono","mode":"stereo"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
    assert_eq!(
        body["error"],
        "Mono Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono."
    );

    // The refused selection was not saved: the output still reports auto.
    let output = listed_output(&app, "alsa#mono").await;
    assert_eq!(output["channel_mode"], serde_json::Value::Null);
}

#[tokio::test]
async fn channel_mode_is_saved_per_output_and_auto_is_reported_as_absent() {
    let (app, state) = mock_app().await;
    state.set_mock_local_outputs(vec![mono_speaker()]);

    // Mono on a mono device is accepted (it already plays mono).
    let (status, _) = send(
        app.clone(),
        json(
            "PUT",
            "/api/outputs/channel-mode",
            r#"{"transport":"local","device_id":"alsa#mono","mode":"mono"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let output = listed_output(&app, "alsa#mono").await;
    assert_eq!(output["channel_mode"], "mono");
    assert_eq!(output["channels"], 1);
    // Other outputs are untouched by one output's mode.
    assert_eq!(
        listed_output(&app, "uuid:mock-sonos").await["channel_mode"],
        serde_json::Value::Null
    );

    // Auto forgets the entry (absent means auto) ...
    let (status, _) = send(
        app.clone(),
        json(
            "PUT",
            "/api/outputs/channel-mode",
            r#"{"transport":"local","device_id":"alsa#mono","mode":"auto"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        listed_output(&app, "alsa#mono").await["channel_mode"],
        serde_json::Value::Null
    );
    assert_eq!(
        state.channel_mode("local", "alsa#mono"),
        on_air_core::dsp::channel::ChannelMode::Auto
    );
}

#[tokio::test]
async fn stereo_on_a_destination_with_unknown_channels_is_accepted() {
    let (app, _state) = mock_app().await;
    // A network receiver whose channel count the core cannot probe: the
    // radio surface is stereo by construction, so stereo fits.
    let (status, _) = send(
        app.clone(),
        json(
            "PUT",
            "/api/outputs/channel-mode",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos","mode":"stereo"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        listed_output(&app, "uuid:mock-sonos").await["channel_mode"],
        "stereo"
    );
}
