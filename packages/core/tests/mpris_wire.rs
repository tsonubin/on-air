//! The requests the MPRIS layer plans are exactly the bodies
//! `POST /api/cd/control` accepts: plan a command the way the bridge
//! does, POST the planned request to the real endpoint, and the deck
//! obeys with the right arguments.

mod common;
use common::*;

use axum::http::StatusCode;
use on_air_core::cd::CdStatus;
use on_air_core::mpris::model::{self, MprisCommand, TrackRef};

/// A mock core with a three-track disc inserted, autoplaying like the
/// drive does.
async fn disc_app() -> (axum::Router, CdStatus) {
    let (app, _) = mock_app().await;
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/mock/cd",
            r#"{"present":true,"album":"Kind of Blue","tracks":[
                {"title":"So What","duration_ms":60000},
                {"title":"Freddie Freeloader","duration_ms":50000},
                {"title":"Blue in Green","duration_ms":40000}]}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    (app.clone(), deck(app).await)
}

async fn deck(app: axum::Router) -> CdStatus {
    let (status, body) = send(app, get("/api/cd")).await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_value(body).unwrap()
}

/// Plan the MPRIS command and feed the planned body to the real endpoint,
/// exactly the request the bridge dispatches.
async fn via_the_control_endpoint(
    app: &axum::Router,
    command: MprisCommand,
    cd: &CdStatus,
) -> CdStatus {
    let request = model::plan(command, cd).expect("command must plan a request");
    let body = serde_json::to_string(&request).unwrap();
    let (status, _) = send(app.clone(), post_json("/api/cd/control", &body)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "POST /api/cd/control rejected the planned body {body}"
    );
    deck(app.clone()).await
}

#[tokio::test]
async fn transport_methods_reach_the_control_endpoint() {
    let (app, cd) = disc_app().await;
    assert!(cd.playing, "insert autoplays");

    let cd = via_the_control_endpoint(&app, MprisCommand::PlayPause, &cd).await;
    assert!(!cd.playing, "PlayPause pauses a playing deck");

    let cd = via_the_control_endpoint(&app, MprisCommand::PlayPause, &cd).await;
    assert!(cd.playing, "PlayPause plays a paused deck");

    let cd = via_the_control_endpoint(&app, MprisCommand::Pause, &cd).await;
    assert!(!cd.playing);

    let cd = via_the_control_endpoint(&app, MprisCommand::Play, &cd).await;
    assert!(cd.playing);

    let cd = via_the_control_endpoint(&app, MprisCommand::Stop, &cd).await;
    assert!(!cd.playing, "Stop pauses the deck in place");
    assert_eq!(cd.track, 1, "Stop does not move tracks");

    let cd = via_the_control_endpoint(&app, MprisCommand::Next, &cd).await;
    assert_eq!((cd.track, cd.title.as_deref()), (2, Some("Freddie Freeloader")));

    let cd = via_the_control_endpoint(&app, MprisCommand::Next, &cd).await;
    assert_eq!(cd.track, 3);
    let cd = via_the_control_endpoint(&app, MprisCommand::Next, &cd).await;
    assert_eq!(cd.track, 3, "Next on the last track stays");

    let cd = via_the_control_endpoint(&app, MprisCommand::Previous, &cd).await;
    assert_eq!(cd.track, 2);
}

#[tokio::test]
async fn seek_and_set_position_reach_the_control_endpoint() {
    let (app, cd) = disc_app().await;
    assert_eq!(cd.position_ms, 0, "the mock deck holds still until seeked");

    let cd = via_the_control_endpoint(&app, MprisCommand::Seek { offset_us: 30_000_000 }, &cd).await;
    assert_eq!(cd.position_ms, 30_000, "Seek lands 30 s in");

    let cd = via_the_control_endpoint(&app, MprisCommand::Seek { offset_us: -10_000_000 }, &cd).await;
    assert_eq!(cd.position_ms, 20_000, "a negative Seek backs up");

    // 12_320 ms is a whole number of CD sectors (75/s), so the deck lands
    // exactly there; other values round down to the sector grid.
    let cd = via_the_control_endpoint(
        &app,
        MprisCommand::SetPosition {
            track: TrackRef::Track(1),
            position_us: 12_320_000,
        },
        &cd,
    )
    .await;
    assert_eq!(cd.position_ms, 12_320, "SetPosition seeks to the absolute spot");

    // Beyond the track's end clamps to it, and `goto`-style stepping is
    // what `Next` plans.
    let cd = via_the_control_endpoint(&app, MprisCommand::Seek { offset_us: 900_000_000 }, &cd).await;
    assert_eq!(cd.position_ms, 60_000, "seek past the end lands at the end");

    let cd = via_the_control_endpoint(&app, MprisCommand::Next, &cd).await;
    assert_eq!(
        (cd.track, cd.position_ms),
        (2, 0),
        "the next track starts at zero"
    );
}

#[tokio::test]
async fn a_foreign_set_position_plans_no_request_at_all() {
    let (app, cd) = disc_app().await;
    // MPRIS: SetPosition for a track that is not current does nothing, so
    // there is nothing to send to the endpoint either.
    assert_eq!(
        model::plan(
            MprisCommand::SetPosition {
                track: TrackRef::Track(3),
                position_us: 1_000_000,
            },
            &cd
        ),
        None
    );
    assert_eq!(
        model::plan(
            MprisCommand::SetPosition {
                track: TrackRef::Foreign,
                position_us: 1_000_000,
            },
            &cd
        ),
        None
    );
    assert_eq!(
        model::plan(
            MprisCommand::SetPosition {
                track: TrackRef::NoTrack,
                position_us: -1,
            },
            &cd
        ),
        None
    );
    let unchanged = deck(app).await;
    assert_eq!(unchanged.position_ms, cd.position_ms);
}
