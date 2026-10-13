//! The client against the real core in mock mode (`CoreState::new_mock`),
//! served in-process on a free loopback port. This is the same core the
//! e2e suites use, so the mirrored JSON types are checked against the
//! serde structs they copy.

use on_air_core::state::CoreState;
use on_air_tui::api::WsEvent;
use on_air_tui::client::{Client, ClientError, Connection, Incoming};
use on_air_tui::error_copy;
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
        "The audio service is paused. Turn it on to continue."
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
