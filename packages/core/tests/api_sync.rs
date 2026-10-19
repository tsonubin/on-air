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
    use on_air_api::SyncState;
    use on_air_core::sync::{DriftReport, OutputKey};

    let (app, state) = mock_app().await;
    let group = state.sync.create_group("Living room");
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
    state.sync.join(&group, &airplay).unwrap();
    state.sync.join(&group, &bluetooth).unwrap();

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

/// `POST /api/sync/groups` and its members routes: the group appears in
/// `GET /api/sync` with its members, and every member's output row knows
/// which group it plays with.
#[tokio::test]
async fn sync_groups_round_trip_over_the_api() {
    let (app, state) = mock_app().await;

    let (status, body) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Downstairs"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Downstairs");
    assert_eq!(body["playing"], false);
    assert_eq!(body["members"].as_array().unwrap().len(), 0);
    let group_id = body["id"].as_u64().unwrap();
    let members_path = format!("/api/sync/groups/{group_id}/members");

    for (transport, device_id) in [("sonos", "uuid:mock-sonos"), ("airplay", "ap-living")] {
        let (status, _) = send(
            app.clone(),
            post_json(
                &members_path,
                &format!(r#"{{"transport":"{transport}","device_id":"{device_id}"}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{transport} joins");
    }
    // Re-adding a member of this same group is a no-op, not an error.
    let (status, _) = send(
        app.clone(),
        post_json(
            &members_path,
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let groups = body["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["id"], group_id);
    let members = groups[0]["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["device_name"], "Mock Sonos");
    assert_eq!(members[0]["playing"], false, "membership is not playback");
    let sonos_row = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "uuid:mock-sonos")
        .unwrap();
    assert_eq!(sonos_row["group_id"], group_id);
    assert_eq!(
        sonos_row["calibrated"], false,
        "grouping never makes a latency figure a measurement"
    );

    let (status, _) = send(
        app.clone(),
        post_json(
            &format!("/api/sync/groups/{group_id}/leave"),
            r#"{"transport":"airplay","device_id":"ap-living"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["groups"][0]["members"].as_array().unwrap().len(), 1);
    let airplay_row = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "ap-living")
        .unwrap();
    assert!(airplay_row.get("group_id").is_none(), "left the group");
    assert_eq!(airplay_row["state"], "free_running");

    let (status, _) = send(
        app.clone(),
        json("DELETE", &format!("/api/sync/groups/{group_id}"), ""),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = send(app, get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["groups"].as_array().unwrap().len(), 0);
    state.shutdown().await;
}

/// An output plays in at most one group at a time: the second join is
/// refused and names the group that already has it.
#[tokio::test]
async fn an_output_cannot_be_in_two_groups_at_once() {
    let (app, _state) = mock_app().await;
    let (_, first) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Downstairs"}"#),
    )
    .await;
    let (_, second) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Party"}"#),
    )
    .await;
    let first_id = first["id"].as_u64().unwrap();
    let second_id = second["id"].as_u64().unwrap();

    let (status, _) = send(
        app.clone(),
        post_json(
            &format!("/api/sync/groups/{first_id}/members"),
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, error) = send(
        app.clone(),
        post_json(
            &format!("/api/sync/groups/{second_id}/members"),
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "conflict");
    assert!(
        error["error"].as_str().unwrap().contains("Downstairs"),
        "the refusal names the group that already has it: {}",
        error["error"]
    );

    // The refused join changed nothing.
    let (_, body) = send(app, get("/api/sync")).await;
    let groups = body["groups"].as_array().unwrap();
    let in_first = groups.iter().find(|group| group["id"] == first_id).unwrap()["members"]
        .as_array()
        .unwrap()
        .len();
    let in_second = groups
        .iter()
        .find(|group| group["id"] == second_id)
        .unwrap()["members"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!((in_first, in_second), (1, 0));
}

/// Starting a group hands every member ONE presentation epoch, scheduled
/// ahead (never fire-now), and every member starts against it.
#[tokio::test]
async fn starting_a_sync_group_begins_every_member_at_one_presentation_epoch() {
    use std::time::Instant;
    let (app, state) = mock_app().await;
    let (_, created) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Downstairs"}"#),
    )
    .await;
    let group_id = created["id"].as_u64().unwrap();
    let members_path = format!("/api/sync/groups/{group_id}/members");
    for (transport, device_id) in [("sonos", "uuid:mock-sonos"), ("airplay", "ap-living")] {
        let (status, _) = send(
            app.clone(),
            post_json(
                &members_path,
                &format!(r#"{{"transport":"{transport}","device_id":"{device_id}"}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    let before = Instant::now();
    let (status, _) = send(
        app.clone(),
        post_json(&format!("/api/sync/groups/{group_id}/start"), ""),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let finished = Instant::now();

    let group = state.sync.group_by_id(group_id).unwrap();
    let epoch = group.epoch().expect("the group has one presentation epoch");
    assert!(
        epoch.start_at >= before + on_air_core::sync::lifecycle::START_PRE_ROLL,
        "the epoch is pre-rolled, not fire-now (before {before:?}, epoch {:?})",
        epoch.start_at
    );
    assert!(
        finished >= epoch.start_at,
        "no member may sound before the shared epoch"
    );

    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let group_view = &body["groups"][0];
    assert_eq!(group_view["playing"], true);
    for member in group_view["members"].as_array().unwrap() {
        assert_eq!(member["playing"], true, "{} started", member["device_name"]);
        assert!(
            member.get("error").is_none(),
            "{} started cleanly",
            member["device_name"]
        );
    }
    let log = state.mock_log.lock().await.clone();
    assert!(log.iter().any(|line| line == "Mock Sonos:start"), "{log:?}");
    assert!(
        log.iter().any(|line| line == "Living Room AirPlay:start"),
        "{log:?}"
    );
    state.shutdown().await;
}

/// Leaving a group stops that member cleanly; the other members keep
/// playing together.
#[tokio::test]
async fn leaving_a_group_stops_that_member_cleanly_and_the_others_keep_playing() {
    let (app, state) = mock_app().await;
    let (_, created) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Downstairs"}"#),
    )
    .await;
    let group_id = created["id"].as_u64().unwrap();
    let members_path = format!("/api/sync/groups/{group_id}/members");
    for (transport, device_id) in [("sonos", "uuid:mock-sonos"), ("airplay", "ap-living")] {
        let (status, _) = send(
            app.clone(),
            post_json(
                &members_path,
                &format!(r#"{{"transport":"{transport}","device_id":"{device_id}"}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (status, _) = send(
        app.clone(),
        post_json(&format!("/api/sync/groups/{group_id}/start"), ""),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send(
        app.clone(),
        post_json(
            &format!("/api/sync/groups/{group_id}/leave"),
            r#"{"transport":"airplay","device_id":"ap-living"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let group_view = &body["groups"][0];
    assert_eq!(group_view["members"].as_array().unwrap().len(), 1);
    assert_eq!(group_view["playing"], true, "the group keeps playing");
    assert_eq!(
        group_view["members"][0]["playing"], true,
        "the remaining member is untouched"
    );
    let left = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "ap-living")
        .unwrap();
    assert_eq!(left["state"], "free_running", "the leaver is out of sync");
    assert!(left.get("group_id").is_none());
    let log = state.mock_log.lock().await.clone();
    assert_eq!(
        log.iter()
            .filter(|line| line.as_str() == "Living Room AirPlay:stop")
            .count(),
        1,
        "the leaver stopped exactly once: {log:?}"
    );
    assert_eq!(
        log.iter()
            .filter(|line| line.as_str() == "Mock Sonos:stop")
            .count(),
        0,
        "the other member never stopped: {log:?}"
    );
    state.shutdown().await;
}

/// The API's guard rails: unknown groups and outputs, bad names, a paused
/// service, and the hard refusal when a patchbay route claims a receiver.
#[tokio::test]
async fn sync_group_requests_validate_and_refuse_conflicts() {
    let (app, state) = mock_app().await;

    let (status, error) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"  "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "invalid_request");

    let (status, error) = send(app.clone(), post_json("/api/sync/groups/999/start", "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["code"], "not_found");

    let (status, error) = send(
        app.clone(),
        post_json(
            "/api/sync/groups/999/members",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown groups hold nothing");
    assert_eq!(error["code"], "not_found");

    let (_, created) = send(
        app.clone(),
        post_json("/api/sync/groups", r#"{"name":"Downstairs"}"#),
    )
    .await;
    let created_id = created["id"].as_u64().unwrap();
    let members_path = format!("/api/sync/groups/{created_id}/members");
    let (status, error) = send(
        app.clone(),
        post_json(
            &members_path,
            r#"{"transport":"carrier-pigeon","device_id":"x"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown outputs cannot join");
    assert_eq!(error["code"], "not_found");

    // A patchbay route owns the receiver: starting the group is refused
    // (two sessions claiming one receiver is a hard refusal) and nothing
    // plays.
    let (_, snapshot) = send(app.clone(), get("/api/patchbay")).await;
    let receiver = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "sonos")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/patchbay",
            &format!(r#"{{"op":"connect","source":"desktop","target":"{receiver}"}}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        app.clone(),
        post_json(
            &members_path,
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, error) = send(
        app.clone(),
        post_json(&format!("/api/sync/groups/{created_id}/start"), ""),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "not_ready");
    let (_, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(
        body["groups"][0]["playing"], false,
        "the refusal started nothing"
    );

    // A paused service refuses to start as well.
    state
        .service_enabled
        .store(false, std::sync::atomic::Ordering::Release);
    let (status, error) = send(
        app.clone(),
        post_json(&format!("/api/sync/groups/{created_id}/start"), ""),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["code"], "service_paused");
    state.shutdown().await;
}
