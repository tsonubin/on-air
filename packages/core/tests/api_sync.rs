//! The sync API: per-output trim and status (trim, drift, coarse state).

use axum::http::StatusCode;
mod common;
use common::*;

#[tokio::test]
async fn sync_trim_round_trips_through_the_status() {
    let (app, _state) = mock_app().await;

    let (status, body) = send(
        app.clone(),
        put_json(
            "/api/sync/trim",
            r#"{"transport":"bluetooth","device_id":"bt-1","trim_ms":-100}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["trim_ms"], -100);
    assert_eq!(
        body["effective_latency_ms"], 50,
        "the 150 ms Bluetooth default minus the 100 ms trim"
    );

    let (status, body) = send(app, get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let outputs = body["outputs"].as_array().unwrap();
    let bt = outputs
        .iter()
        .find(|output| output["device_id"] == "bt-1")
        .expect("the trimmed output is reported");
    assert_eq!(bt["trim_ms"], -100);
    assert_eq!(bt["default_latency_ms"], 150);
    assert_eq!(bt["effective_latency_ms"], 50);
    assert_eq!(
        bt["calibrated"], false,
        "a trim is manual calibration, never a measurement"
    );
    assert_eq!(
        bt["state"], "free_running",
        "outside a sync group an output free-runs"
    );
    assert_eq!(bt["delay_frames"], 0);
    assert!(
        bt.get("drift_ppm").is_none(),
        "no window measured means no drift figure"
    );
}

#[tokio::test]
async fn sync_trim_validates_range_and_transport() {
    let (app, _state) = mock_app().await;
    for body in [
        r#"{"transport":"bluetooth","device_id":"x","trim_ms":20000}"#,
        r#"{"transport":"carrier-pigeon","device_id":"x","trim_ms":10}"#,
    ] {
        let (status, response) = send(app.clone(), put_json("/api/sync/trim", body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["code"], "invalid_request");
    }
}

#[tokio::test]
async fn a_sync_group_member_reports_its_delay_line_state_and_drift() {
    use on_air_core::sync::{DriftReport, OutputKey};
    use on_air_api::SyncState;

    let (app, state) = mock_app().await;
    let group = state.sync.create_group();
    let airplay = on_air_core::state::ActiveOutput {
        transport: "airplay".into(),
        device_id: "ap-1".into(),
        device_name: "Living Room".into(),
    };
    let bluetooth = on_air_core::state::ActiveOutput {
        transport: "bluetooth".into(),
        device_id: "bt-1".into(),
        device_name: "Patio".into(),
    };
    state.sync.join(&group, &airplay);
    state.sync.join(&group, &bluetooth);

    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let outputs = body["outputs"].as_array().unwrap();
    let slow = outputs
        .iter()
        .find(|output| output["device_id"] == "ap-1")
        .unwrap();
    let fast = outputs
        .iter()
        .find(|output| output["device_id"] == "bt-1")
        .unwrap();
    assert_eq!(slow["state"], "syncing", "joined, not yet measured");
    assert_eq!(
        slow["delay_frames"], 0,
        "the slowest member waits for nobody"
    );
    assert!(
        fast["delay_frames"].as_u64().unwrap() > 0,
        "the fast member holds the timeline back 1850 ms, got {}",
        fast["delay_frames"]
    );

    // A feed's measurement turns the state into a drift figure.
    state.sync.report_drift(
        &bluetooth,
        Some(DriftReport {
            drift_ppm: 42.0,
            state: SyncState::Locked,
        }),
    );
    let (status, body) = send(app, get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let fast = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "bt-1")
        .unwrap();
    assert_eq!(fast["state"], "locked");
    assert_eq!(fast["drift_ppm"], 42.0);

    // Leaving the group free-runs again, with no delay line.
    let _ = state.sync.leave(&OutputKey::new("bluetooth", "bt-1"));
}
