//! Real D-Bus round trips for the MPRIS bridge, against a private
//! `dbus-daemon` per test (so parallel tests never fight over the
//! well-known name). Every assertion here crossed the actual session-bus
//! protocol: method calls, property reads, signals, name ownership.
//! Without `dbus-daemon` on the machine the tests skip themselves.

#![cfg(target_os = "linux")]

mod common;
use common::*;

use axum::http::StatusCode;
use futures_util::StreamExt;
use on_air_core::mpris::{spawn_on, OBJECT_PATH, SERVICE_NAME};
use std::collections::HashMap;
use std::io::BufRead;
use std::time::Duration;

const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

struct PrivateBus {
    child: std::process::Child,
    address: String,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// A private session bus. `None` skips the test on a host without a
/// `dbus-daemon` binary.
fn private_bus() -> Option<PrivateBus> {
    let mut child = std::process::Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut address = String::new();
    std::io::BufReader::new(stdout)
        .read_line(&mut address)
        .ok()?;
    let address = address.trim().to_string();
    (!address.is_empty()).then_some(PrivateBus { child, address })
}

async fn client(address: &str) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .build()
        .await
        .expect("connect to the private bus")
}

async fn bus_names(conn: &zbus::Connection) -> Vec<String> {
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListNames",
            &(),
        )
        .await
        .expect("ListNames");
    let (names,): (Vec<String>,) = reply.body().deserialize().expect("ListNames body");
    names
}

async fn wait_for_name(conn: &zbus::Connection, owned: bool) {
    for _ in 0..400 {
        let names = bus_names(conn).await;
        if names.iter().any(|name| name == SERVICE_NAME) == owned {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "the bridge never {} {SERVICE_NAME}",
        if owned { "owned" } else { "released" }
    );
}

async fn wait_for_names(conn: &zbus::Connection, want: &[String]) {
    for _ in 0..400 {
        let mut names = bus_names(conn).await;
        names.sort();
        let mut want = want.to_vec();
        want.sort();
        if names == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the bus never settled at the expected names");
}

async fn get_property(conn: &zbus::Connection, interface: &str, property: &str) -> zbus::OwnedVariant {
    let reply = conn
        .call_method(
            Some(SERVICE_NAME),
            OBJECT_PATH,
            Some(PROPERTIES_INTERFACE),
            "Get",
            &(interface, property),
        )
        .await
        .unwrap_or_else(|error| panic!("Get {interface}.{property}: {error}"));
    let (value,): (zbus::OwnedVariant,) = reply.body().deserialize().expect("Get body");
    value
}

async fn set(
    conn: &zbus::Connection,
    interface: &str,
    property: &str,
    value: zbus::Variant<'_>,
) {
    conn.call_method(
        Some(SERVICE_NAME),
        OBJECT_PATH,
        Some(PROPERTIES_INTERFACE),
        "Set",
        &(interface, property, value),
    )
    .await
    .expect("Set");
}

async fn call(
    conn: &zbus::Connection,
    method: &str,
    body: &(impl serde::Serialize + zbus::DynamicType),
) {
    conn.call_method(
        Some(SERVICE_NAME),
        OBJECT_PATH,
        Some(PLAYER_INTERFACE),
        method,
        body,
    )
    .await
    .unwrap_or_else(|error| panic!("{method}: {error}"));
}

fn text(value: &zbus::OwnedVariant) -> String {
    String::try_from(value.try_clone().unwrap()).unwrap()
}

fn object_path(value: &zbus::OwnedVariant) -> String {
    zbus::OwnedObjectPath::try_from(value.try_clone().unwrap())
        .unwrap()
        .as_str()
        .to_string()
}

fn flag(value: &zbus::OwnedVariant) -> bool {
    bool::try_from(value).unwrap()
}

fn integer(value: &zbus::OwnedVariant) -> i64 {
    i64::try_from(value)
        .or_else(|_| i32::try_from(value).map(i64::from))
        .unwrap()
}

fn real(value: &zbus::OwnedVariant) -> f64 {
    f64::try_from(value).unwrap()
}

fn table(value: &zbus::OwnedVariant) -> HashMap<String, zbus::OwnedVariant> {
    HashMap::try_from(value.try_clone().unwrap()).unwrap()
}

fn list(value: &zbus::OwnedVariant) -> Vec<String> {
    Vec::try_from(value.try_clone().unwrap()).unwrap()
}

/// The mock core behind its router, its private bus kept alive by the
/// runtime, and the mock Sonos output activated so the face exists before
/// any disc does. The returned state is the one the router serves: the
/// bridge must watch that same core.
async fn served() -> (zbus::Connection, axum::Router, String, on_air_core::state::CoreState) {
    let Some(bus) = private_bus() else {
        panic!("skipped: no dbus-daemon on this host");
    };
    let address = bus.address.clone();
    let conn = client(&address).await;
    let (app, state) = mock_app().await;
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "mock output activates");
    tokio::spawn(async move {
        let _bus = bus;
        std::future::pending::<()>().await;
    });
    (conn, app, address, state)
}

async fn insert_disc(app: &axum::Router) {
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
}

async fn eject_disc(app: &axum::Router) {
    let (status, _) = send(
        app.clone(),
        post_json("/api/mock/cd", r#"{"present":false}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn cd(app: &axum::Router) -> serde_json::Value {
    let (status, body) = send(app.clone(), get("/api/cd")).await;
    assert_eq!(status, StatusCode::OK);
    body
}

async fn wait_for(
    conn: &zbus::Connection,
    interface: &str,
    property: &str,
    mut satisfied: impl FnMut(&zbus::OwnedVariant) -> bool,
) {
    for _ in 0..400 {
        if satisfied(&get_property(conn, interface, property).await) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{interface}.{property} never reached the wanted value");
}

async fn signal_stream(
    conn: &zbus::Connection,
    interface: &str,
    member: &str,
) -> zbus::MessageStream {
    let rule = zbus::MatchRule::builder()
        .interface(interface)
        .member(member)
        .path(OBJECT_PATH)
        .build()
        .expect("match rule");
    zbus::MessageStream::for_match_rule(rule, conn, None)
        .await
        .expect("signal stream")
}

/// The next `Seeked` position, in microseconds.
async fn next_seeked(stream: &mut zbus::MessageStream) -> i64 {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("no Seeked signal arrived")
            .expect("Seeked stream closed")
            .expect("Seeked stream error");
        if let Ok((position,)) = message.body().deserialize::<(i64,)>() {
            return position;
        }
    }
}

/// The announced property names of the next `PropertiesChanged` that
/// mentions `wanted`.
async fn next_changed(stream: &mut zbus::MessageStream, wanted: &str) -> Vec<String> {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("no PropertiesChanged signal arrived")
            .expect("PropertiesChanged stream closed")
            .expect("PropertiesChanged stream error");
        let body = message.body();
        if let Ok((_, changed, _)) = body
            .deserialize::<(String, HashMap<String, zbus::OwnedVariant>, Vec<String>)>()
        {
            let keys: Vec<String> = changed.keys().cloned().collect();
            if keys.iter().any(|key| key == wanted) {
                return keys;
            }
        }
    }
}

#[tokio::test]
async fn idle_bridge_holds_no_bus_connection_until_a_disc_appears() {
    let Some(bus) = private_bus() else {
        eprintln!("skipped: no dbus-daemon on this host");
        return;
    };
    let conn = client(&bus.address).await;
    let (app, state) = mock_app().await;
    let baseline = bus_names(&conn).await;

    let _bridge = spawn_on(state, bus.address.clone());
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        bus_names(&conn).await,
        baseline,
        "with no disc and no output the bridge holds no connection at all: \
         no unique name, no {SERVICE_NAME}"
    );

    insert_disc(&app).await;
    wait_for_name(&conn, true).await;
    assert_eq!(
        text(&get_property(&conn, PLAYER_INTERFACE, "PlaybackStatus").await),
        "Playing"
    );
    assert_eq!(text(&get_property(&conn, ROOT_INTERFACE, "Identity").await), "on-air");

    eject_disc(&app).await;
    wait_for_name(&conn, false).await;
    wait_for_names(&conn, &baseline).await;
}

#[tokio::test]
async fn properties_and_methods_follow_the_contract() {
    let (conn, app, address, state) = served().await;
    let _bridge = spawn_on(state, address);
    wait_for_name(&conn, true).await;

    // Root: exactly what docs/omarchy.md "Media" demands.
    assert_eq!(text(&get_property(&conn, ROOT_INTERFACE, "Identity").await), "on-air");
    assert_eq!(
        text(&get_property(&conn, ROOT_INTERFACE, "DesktopEntry").await),
        "on-air"
    );
    assert!(flag(&get_property(&conn, ROOT_INTERFACE, "CanRaise").await));
    assert!(!flag(&get_property(&conn, ROOT_INTERFACE, "CanQuit").await));
    assert!(!flag(&get_property(&conn, ROOT_INTERFACE, "HasTrackList").await));
    assert!(list(&get_property(&conn, ROOT_INTERFACE, "SupportedUriSchemes").await).is_empty());
    assert!(list(&get_property(&conn, ROOT_INTERFACE, "SupportedMimeTypes").await).is_empty());

    // No disc yet: Stopped, no metadata, nothing to control.
    assert_eq!(
        text(&get_property(&conn, PLAYER_INTERFACE, "PlaybackStatus").await),
        "Stopped"
    );
    assert!(table(&get_property(&conn, PLAYER_INTERFACE, "Metadata").await).is_empty());
    assert!(!flag(&get_property(&conn, PLAYER_INTERFACE, "CanGoNext").await));
    assert!(!flag(&get_property(&conn, PLAYER_INTERFACE, "CanPlay").await));
    assert!(!flag(&get_property(&conn, PLAYER_INTERFACE, "CanSeek").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanControl").await));

    // Rates are fixed at 1.
    assert_eq!(real(&get_property(&conn, PLAYER_INTERFACE, "Rate").await), 1.0);
    assert_eq!(real(&get_property(&conn, PLAYER_INTERFACE, "MinimumRate").await), 1.0);
    assert_eq!(real(&get_property(&conn, PLAYER_INTERFACE, "MaximumRate").await), 1.0);

    // Volume maps 0..=1 to the active output's percent.
    assert_eq!(real(&get_property(&conn, PLAYER_INTERFACE, "Volume").await), 0.5);
    set(
        &conn,
        PLAYER_INTERFACE,
        "Volume",
        zbus::Variant::from(0.42f64),
    )
    .await;
    let (status, body) = send(app.clone(), get("/api/outputs/active/volume")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["volume"], 42,
        "Volume reaches POST /api/outputs/active/volume"
    );
    assert_eq!(real(&get_property(&conn, PLAYER_INTERFACE, "Volume").await), 0.42);

    // A disc appears: Playing and full metadata from GET /api/cd.
    insert_disc(&app).await;
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Playing"
    })
    .await;
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanGoNext").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanGoPrevious").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanPlay").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanPause").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanSeek").await));
    let metadata = table(&get_property(&conn, PLAYER_INTERFACE, "Metadata").await);
    assert_eq!(text(&metadata["xesam:title"]), "So What");
    assert_eq!(text(&metadata["xesam:album"]), "Kind of Blue");
    assert_eq!(integer(&metadata["xesam:trackNumber"]), 1);
    assert_eq!(integer(&metadata["mpris:length"]), 60_000_000, "length is µs");
    assert_eq!(
        object_path(&metadata["mpris:trackid"]),
        "/org/on_air/track/1",
        "a trackid per track"
    );
    assert!(
        !metadata.contains_key("mpris:artUrl"),
        "artUrl only if cover art exists, and none does"
    );
    assert_eq!(integer(&get_property(&conn, PLAYER_INTERFACE, "Position").await), 0);

    // Methods cross the wire and drive the deck through the same path
    // `POST /api/cd/control` takes.
    call(&conn, "Next", &()).await;
    assert_eq!(cd(&app).await["track"], 2);

    // 12_320 ms is a whole number of CD sectors (75/s), so the deck lands
    // exactly there; other values round down to the sector grid.
    let track_two = zbus::OwnedObjectPath::try_from("/org/on_air/track/2").unwrap();
    call(&conn, "SetPosition", &(track_two, 12_320i64 * 1_000)).await;
    let status = cd(&app).await;
    assert_eq!(status["track"], 2);
    assert_eq!(status["position_ms"], 12_320);

    let track_three = zbus::OwnedObjectPath::try_from("/org/on_air/track/3").unwrap();
    call(&conn, "SetPosition", &(track_three, 1_000_000i64)).await;
    assert_eq!(
        cd(&app).await["position_ms"],
        12_320,
        "SetPosition for another track does nothing"
    );

    call(&conn, "Seek", &(5i64 * 1_000_000)).await;
    assert_eq!(cd(&app).await["position_ms"], 17_320, "Seek is relative");

    call(&conn, "Pause", &()).await;
    assert_eq!(cd(&app).await["playing"], false);
    call(&conn, "Play", &()).await;
    assert_eq!(cd(&app).await["playing"], true);
    call(&conn, "PlayPause", &()).await;
    assert_eq!(cd(&app).await["playing"], false);
    call(&conn, "Stop", &()).await;
    assert_eq!(cd(&app).await["playing"], false);

    // `Previous` past the pre-gap restarts the track, and from its start
    // steps back a track — the deck's own semantics, reached unchanged.
    call(&conn, "Previous", &()).await;
    let status = cd(&app).await;
    assert_eq!(status["track"], 2);
    assert_eq!(status["position_ms"], 0);
    call(&conn, "Previous", &()).await;
    let status = cd(&app).await;
    assert_eq!(status["track"], 1);
    assert_eq!(status["position_ms"], 0);
}

#[tokio::test]
async fn playback_status_transitions_across_enabled_paused_and_empty() {
    let (conn, app, address, state) = served().await;
    let _bridge = spawn_on(state, address);
    wait_for_name(&conn, true).await;

    // Empty: the service is on and no source is live.
    assert_eq!(
        text(&get_property(&conn, PLAYER_INTERFACE, "PlaybackStatus").await),
        "Stopped"
    );

    // Enabled with a source live.
    insert_disc(&app).await;
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Playing"
    })
    .await;

    // The service is paused via PUT /api/service.
    let (status, _) = send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":false}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Paused"
    })
    .await;

    // Back on and playing again...
    let (status, _) = send(
        app.clone(),
        put_json("/api/service", r#"{"enabled":true}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    call(&conn, "Play", &()).await;
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Playing"
    })
    .await;

    // ...then the deck stops with the disc still in and the service on:
    // the contract's third clause.
    call(&conn, "Pause", &()).await;
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Stopped"
    })
    .await;

    // Empty again: the media is gone. Re-activate the mock output — a mock
    // core has no saved session to restore after the service pause — it is
    // the face's other leg (the widget's "no disc and no active output"
    // rule).
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/outputs/active",
            r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    eject_disc(&app).await;
    wait_for(&conn, PLAYER_INTERFACE, "PlaybackStatus", |value| {
        text(value) == "Stopped"
    })
    .await;
    assert!(table(&get_property(&conn, PLAYER_INTERFACE, "Metadata").await).is_empty());
}

#[tokio::test]
async fn seeked_and_property_signals_track_the_deck() {
    let (conn, app, address, state) = served().await;
    let _bridge = spawn_on(state, address);
    wait_for_name(&conn, true).await;

    let mut seeked = signal_stream(&conn, PLAYER_INTERFACE, "Seeked").await;
    let mut changed = signal_stream(&conn, PROPERTIES_INTERFACE, "PropertiesChanged").await;

    insert_disc(&app).await;
    // A discontinuity the bridge did not cause: POST /api/cd/control from
    // outside, 30 s forward while the deck plays.
    let (status, _) = send(
        app.clone(),
        post_json("/api/cd/control", r#"{"action":"seek","position_ms":30000}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let landed = cd(&app).await;
    let landed = landed["position_ms"].as_u64().unwrap() as i64 * 1_000;
    assert_eq!(
        next_seeked(&mut seeked).await,
        landed,
        "Seeked carries the deck's new position in µs"
    );
    assert_eq!(landed, 30_000_000);

    // And one the bridge does cause, tracked to the deck the same way.
    // 9_880 ms is a whole number of CD sectors, so the deck lands exactly.
    let track_one = zbus::OwnedObjectPath::try_from("/org/on_air/track/1").unwrap();
    call(&conn, "SetPosition", &(track_one, 9_880i64 * 1_000)).await;
    let landed = cd(&app).await;
    let landed = landed["position_ms"].as_u64().unwrap() as i64 * 1_000;
    assert_eq!(next_seeked(&mut seeked).await, landed);
    assert_eq!(landed, 9_880_000);

    // PropertiesChanged announces the flags when the disc leaves. The
    // output is still active, so the name survives the eject and the flip
    // is observable.
    eject_disc(&app).await;
    wait_for(&conn, PLAYER_INTERFACE, "CanGoNext", |value| !flag(value)).await;
    assert!(!flag(&get_property(&conn, PLAYER_INTERFACE, "CanPlay").await));
    assert!(!flag(&get_property(&conn, PLAYER_INTERFACE, "CanSeek").await));
    assert!(flag(&get_property(&conn, PLAYER_INTERFACE, "CanControl").await));
    assert!(table(&get_property(&conn, PLAYER_INTERFACE, "Metadata").await).is_empty());
    let announced = next_changed(&mut changed, "CanGoNext").await;
    assert!(
        announced.iter().any(|key| key == "Metadata"),
        "one PropertiesChanged covers the flip, got {announced:?}"
    );
}
