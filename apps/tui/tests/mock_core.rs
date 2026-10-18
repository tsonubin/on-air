//! The client against the real core in mock mode (`CoreState::new_mock`),
//! served in-process on a free loopback port. This is the same core the
//! e2e suites use, so the mirrored JSON types are checked against the
//! serde structs they copy.

use on_air_core::state::CoreState;
use on_air_tui::api::{CdAction, DspQuality, WsEvent};
use on_air_tui::client::{Client, ClientError, Connection, Incoming};
use on_air_tui::error_copy;
use on_air_tui::update::{Cmd, Msg};
use std::time::Duration;

async fn mock_core() -> Client {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = CoreState::new_mock().await;
    tokio::spawn(async move {
        on_air_core::serve_with_state(listener, state)
            .await
            .unwrap();
    });
    Client::new(&format!("http://{addr}"))
}

#[tokio::test]
async fn golden_path_sonos() {
    let c = mock_core().await;
    assert_eq!(c.status().await.unwrap().status, "ok");
    assert_eq!(c.pin().await.unwrap(), "123456");
    let inputs = c.inputs().await.unwrap();
    assert_eq!(inputs, vec!["Mock Monitor"]);
    c.activate_input("Mock Monitor").await.unwrap();
    assert_eq!(
        c.active_input().await.unwrap().name.as_deref(),
        Some("Mock Monitor")
    );
    let outputs = c.outputs().await.unwrap();
    let sonos = outputs.iter().find(|o| o.transport == "sonos").unwrap();
    c.activate_output("sonos", &sonos.id).await.unwrap();
    assert_eq!(
        c.active_output().await.unwrap().unwrap().output.device_id,
        "uuid:mock-sonos"
    );
    c.set_volume(20).await.unwrap();
    assert_eq!(c.volume().await.unwrap(), 20);
    c.set_eq([3.0, 0.0, 0.0, 0.0, -3.0]).await.unwrap();
    assert_eq!(c.eq().await.unwrap(), [3.0, 0.0, 0.0, 0.0, -3.0]);
    let rates = c.sample_rate().await.unwrap();
    assert!(rates
        .input
        .supported_hz
        .contains(&rates.input.sample_rate_hz));
    c.deactivate_output().await.unwrap();
    assert!(c.active_output().await.unwrap().is_none());
    assert!(!c.cd().await.unwrap().present);
    assert!(!c.bluetooth_devices().await.unwrap().is_empty());
    assert!(!c.airplay_mode().await.unwrap().is_empty());
}

#[tokio::test]
async fn service_off_is_reported_and_blocks_audio() {
    let c = mock_core().await;
    assert!(!c.set_service(false).await.unwrap());
    assert!(!c.status().await.unwrap().service_enabled);
    let err = c.activate_input("Mock Monitor").await.unwrap_err();
    assert!(
        matches!(err, ClientError::Http { status: 503, .. }),
        "{err:?}"
    );
    assert_eq!(
        error_copy::copy(&err),
        "The monitor is off. Turn it on with m or `on-air monitor on`."
    );
    assert!(c.set_service(true).await.unwrap());
}

#[tokio::test]
async fn unreachable_core_is_a_named_error() {
    let c = Client::new("http://127.0.0.1:1");
    let err = c.status().await.unwrap_err();
    assert!(matches!(err, ClientError::Unreachable(_)), "{err:?}");
    assert!(error_copy::copy(&err).starts_with("Core unreachable on http://127.0.0.1:1;"));
}

#[tokio::test]
async fn events_arrive_after_a_service_change() {
    let c = mock_core().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = on_air_tui::client::spawn_events(c.base(), tx);
    assert!(matches!(
        rx.recv().await,
        Some(Incoming::Connection(Connection::Connecting))
    ));
    assert!(matches!(
        rx.recv().await,
        Some(Incoming::Connection(Connection::Open))
    ));
    c.set_service(false).await.unwrap();
    let enabled = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Incoming::Event(WsEvent::ServiceStateChanged { enabled })) = rx.recv().await
            {
                break enabled;
            }
        }
    })
    .await
    .expect("a ServiceStateChanged event within 5 s");
    assert!(!enabled);
    handle.abort();
}

#[tokio::test]
async fn a_snapshot_loads_every_part_from_the_mock_core() {
    let c = mock_core().await;
    let (snap, failures) = c.load_snapshot().await;
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(snap.pin.as_deref(), Some("123456"));
    assert_eq!(snap.inputs, vec!["Mock Monitor"]);
    assert_eq!(
        snap.outputs
            .iter()
            .filter(|o| o.transport == "bluetooth")
            .count(),
        2
    );
    assert_eq!(snap.volume, 50);
    assert!(snap.rates.is_some());
    assert_eq!(snap.cd.as_ref().map(|cd| cd.present), Some(false));
    // The core picks the AirPlay strategy per platform.
    let expected_mode = if cfg!(target_os = "macos") {
        "avroute-picker"
    } else {
        "owntone"
    };
    assert_eq!(snap.airplay_mode.as_deref(), Some(expected_mode));
}

#[tokio::test]
async fn a_paused_core_still_yields_the_parts_that_load() {
    let c = mock_core().await;
    c.set_service(false).await.unwrap();
    let (snap, failures) = c.load_snapshot().await;
    assert_eq!(snap.status.as_ref().map(|s| s.service_enabled), Some(false));
    assert!(
        failures.is_empty(),
        "a paused service is a state the header shows, not an error: {failures:?}"
    );
    assert!(snap.inputs.is_empty());
}

#[tokio::test]
async fn run_cmd_round_trips_an_activation() {
    let c = mock_core().await;
    let msgs = c
        .run_cmd(Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into(),
        })
        .await;
    assert!(
        matches!(msgs.as_slice(), [Msg::Done { label, result: Ok(()) }] if label == "Switching output"),
        "{msgs:?}"
    );
    let msgs = c
        .run_cmd(Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "nope".into(),
        })
        .await;
    assert!(
        matches!(msgs.as_slice(), [Msg::Done { result: Err(copy), .. }] if copy == "That device is no longer available. Refresh devices and try again."),
        "{msgs:?}"
    );
    let msgs = c.run_cmd(Cmd::Refresh).await;
    assert!(
        matches!(msgs.as_slice(), [Msg::Snapshot(Ok(snap))] if snap.active_output.is_some()),
        "{msgs:?}"
    );
    assert!(c.run_cmd(Cmd::Quit).await.is_empty());
    let down = Client::new("http://127.0.0.1:1");
    let msgs = down.run_cmd(Cmd::Refresh).await;
    assert!(
        matches!(msgs.as_slice(), [Msg::Snapshot(Err(copy))] if copy.starts_with("Core unreachable")),
        "{msgs:?}"
    );
}

#[tokio::test]
async fn bar_and_monitor_subcommands_against_the_mock_core() {
    let c = mock_core().await;
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "idle");
    assert_eq!(
        on_air_tui::cli::monitor(&c, "toggle").await.unwrap(),
        "monitor off"
    );
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "off");
    assert_eq!(
        on_air_tui::cli::monitor(&c, "on").await.unwrap(),
        "monitor on"
    );
    c.activate_output("sonos", "uuid:mock-sonos").await.unwrap();
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "live");
    assert_eq!(line["text"], "󰝚 Mock Sonos");
    let down: serde_json::Value =
        serde_json::from_str(&on_air_tui::cli::bar(&Client::new("http://127.0.0.1:1")).await)
            .unwrap();
    assert_eq!(down["class"], "down");
    let status = on_air_tui::cli::status(&c, true).await.unwrap();
    assert!(status.contains("\"service_enabled\""), "{status}");
    let human = on_air_tui::cli::status(&c, false).await.unwrap();
    assert!(human.contains("monitor: on"), "{human}");
    assert!(on_air_tui::cli::monitor(&c, "maybe").await.is_err());
}

#[tokio::test]
async fn a_tui_started_while_paused_still_gets_live_events() {
    let c = mock_core().await;
    c.set_service(false).await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = on_air_tui::client::spawn_events(c.base(), tx);
    let opened = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Incoming::Connection(Connection::Open)) = rx.recv().await {
                break;
            }
        }
    })
    .await;
    assert!(opened.is_ok(), "the event socket opens while paused");
    c.set_service(true).await.unwrap();
    let enabled = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Incoming::Event(WsEvent::ServiceStateChanged { enabled })) = rx.recv().await
            {
                break enabled;
            }
        }
    })
    .await
    .expect("the TUI hears the service come back on");
    assert!(enabled);
    handle.abort();
}

#[tokio::test]
async fn snapshot_includes_the_bluetooth_list() {
    let c = mock_core().await;
    let (snapshot, _) = c.load_snapshot().await;
    let list = snapshot.bluetooth.expect("the list loaded");
    assert!(list.iter().any(|d| d.id == "bt-unpaired"), "{list:?}");
}

#[tokio::test]
async fn bluetooth_pair_connect_activate() {
    use on_air_tui::model::{Modal, Model, Pane};
    use on_air_tui::update::update;

    let c = mock_core().await;
    let (snapshot, _) = c.load_snapshot().await;
    let mut model = Model::new();
    model.snapshot = snapshot;
    model.pane = Pane::Outputs;
    let key = |code| {
        Msg::Key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ))
    };
    while model.selected_dest().map(|e| e.id).as_deref() != Some("bt-unpaired") {
        update(&mut model, key(crossterm::event::KeyCode::Char('j')));
    }
    assert!(update(&mut model, key(crossterm::event::KeyCode::Enter)).is_empty());
    assert!(matches!(model.modal, Modal::Pin { .. }));
    let mut cmds = update(&mut model, key(crossterm::event::KeyCode::Enter));
    let mut seen = Vec::new();
    while let Some(cmd) = cmds.pop() {
        seen.push(cmd.clone());
        let mut next = Vec::new();
        for msg in c.run_cmd(cmd).await {
            next.extend(update(&mut model, msg));
        }
        cmds = next;
        if matches!(seen.last(), Some(Cmd::ActivateOutput { .. })) {
            break;
        }
    }
    assert_eq!(
        seen,
        vec![
            Cmd::PairBluetooth("bt-unpaired".into()),
            Cmd::ConnectBluetooth("bt-unpaired".into()),
            Cmd::ActivateOutput {
                transport: "bluetooth".into(),
                device_id: "bt-unpaired".into()
            },
        ]
    );
    assert_eq!(model.error, None);
    let active = c.active_output().await.unwrap().unwrap();
    assert_eq!(active.output.transport, "bluetooth");
    assert_eq!(active.output.device_id, "bt-unpaired");
}

#[tokio::test]
async fn cd_seek_goto_eject() {
    let c = mock_core().await;
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("{}/api/mock/cd", c.base()))
        .json(&serde_json::json!({
            "present": true,
            "album": "Test",
            "tracks": [
                { "title": "One", "duration_ms": 180_000 },
                { "title": "Two", "duration_ms": 200_000 },
            ],
        }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert!(c.cd().await.unwrap().present);
    c.control_cd(CdAction::Seek, Some(1000), None)
        .await
        .unwrap();
    c.control_cd(CdAction::Goto, None, Some(2)).await.unwrap();
    assert_eq!(c.cd().await.unwrap().track, 2);
    c.control_cd(CdAction::Eject, None, None).await.unwrap();
    assert!(!c.cd().await.unwrap().present);
}

#[tokio::test]
async fn the_tuning_mirror_types_round_trip_against_the_real_core() {
    let c = mock_core().await;
    let tuning = c.tuning_status().await.unwrap();
    assert!(!tuning.tuned, "no check has run on this machine");
    assert_eq!(tuning.quality, DspQuality::Economy);
    assert_eq!(tuning.quality_override, None);
    assert_eq!(tuning.profiles, DspQuality::ALL.to_vec());
    c.set_audio_quality(DspQuality::High).await.unwrap();
    let tuning = c.tuning_status().await.unwrap();
    assert_eq!(tuning.quality, DspQuality::High);
    assert_eq!(tuning.quality_override, Some(DspQuality::High));
}
