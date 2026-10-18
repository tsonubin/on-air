use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use on_air_core::sender::sonos::discovery::SonosDevice;
use on_air_core::sender::sonos::SonosSender;
use on_air_core::sender::AudioSender;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>); // soapaction values, in call order

async fn capture_handler(
    State(state): State<Captured>,
    headers: HeaderMap,
    _body: String,
) -> &'static str {
    let action = headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    state.0.lock().unwrap().push(action);
    "OK"
}

async fn start_fake_sonos() -> (std::net::SocketAddr, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/MediaRenderer/AVTransport/Control", post(capture_handler))
        .route(
            "/MediaRenderer/RenderingControl/Control",
            post(capture_handler),
        )
        .with_state(captured.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, captured)
}

#[tokio::test]
async fn start_calls_set_uri_then_play_stop_calls_stop_set_volume_calls_set_volume() {
    let (addr, captured) = start_fake_sonos().await;
    let device = SonosDevice::discovered(
        "uuid:fake",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Fake Speaker",
    );

    let mut sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://192.168.1.10:47990/stream/audio.wav".to_string(),
    );

    sender.start().await.unwrap();
    sender.set_volume(60).await.unwrap();
    sender.stop().await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert!(calls[0].contains("SetAVTransportURI"));
    assert!(calls[1].contains("#Play"));
    assert!(calls[2].contains("SetVolume"));
    assert!(calls[3].contains("#Stop"));
}

#[derive(Clone, Default)]
struct FailingStopProbe(Arc<Mutex<Vec<String>>>);

async fn fail_stop_handler(State(probe): State<FailingStopProbe>, headers: HeaderMap) -> Response {
    let action = headers
        .get("soapaction")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    probe.0.lock().unwrap().push(action.clone());
    if action.contains("#Stop") {
        (StatusCode::INTERNAL_SERVER_ERROR, "stop failed").into_response()
    } else {
        "OK".into_response()
    }
}

#[tokio::test]
async fn failed_stop_preserves_automatic_recovery_for_the_active_sender() {
    use on_air_core::state::{ActiveOutput, CoreState};

    let probe = FailingStopProbe::default();
    let app = Router::new()
        .route(
            "/MediaRenderer/AVTransport/Control",
            post(fail_stop_handler),
        )
        .route(
            "/MediaRenderer/RenderingControl/Control",
            post(fail_stop_handler),
        )
        .with_state(probe.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let device = SonosDevice::discovered(
        "uuid:failed-stop",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Failed Stop",
    );
    let mut state = CoreState::new();
    state.set_mock(true);
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://127.0.0.1:47990/stream/audio.wav".into(),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone());
    state
        .activate_sender_as(
            Box::new(sender),
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:failed-stop".into(),
                device_name: "Failed Stop".into(),
            }),
        )
        .await
        .unwrap();

    assert!(state.deactivate_sender().await.is_err());
    assert!(state.output().sender_name().await.is_some());
    let recovered = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let play_count = probe
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|action| action.contains("#Play"))
                .count();
            if play_count >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        recovered.is_ok(),
        "failed Stop left the preserved Sonos sender without recovery"
    );
}

/// B9 invariant: the stall recovery of a sync-group member must not restart
/// playback at whatever wall-clock instant the stall was noticed. It
/// re-enters the group instead: the restart is held to the group's pending
/// presentation epoch, and from the moment of re-entry the sync status
/// reports `syncing` (the stale drift report is dropped) until the drift
/// servo re-locks.
#[tokio::test]
async fn sync_member_recovery_holds_to_the_group_epoch_and_reports_syncing() {
    use on_air_api::SyncState;
    use on_air_core::state::{ActiveOutput, CoreState};
    use on_air_core::sync::DriftReport;
    use std::time::{Duration, Instant};

    let (addr, captured) = start_fake_sonos().await;
    let device = SonosDevice::discovered(
        "uuid:sync-recovery",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Sync Recovery",
    );
    let identity = ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:sync-recovery".into(),
        device_name: "Sync Recovery".into(),
    };
    let mut state = CoreState::new();
    state.set_mock(true);

    // The output is in a sync group whose coordinated start is still
    // pending, and its drift servo had locked before the stall.
    let group = state.sync.create_group("Recovery");
    state.sync.join(&group, &identity).unwrap();
    let epoch = Instant::now() + Duration::from_millis(4500);
    group.schedule_start(epoch);
    state.sync.report_drift(
        &identity,
        Some(DriftReport {
            drift_ppm: 0.0,
            state: SyncState::Locked,
        }),
    );
    let sync_state = |state: &CoreState| {
        state
            .sync
            .sync_status(&[(identity.clone(), 44_100)])
            .into_iter()
            .find(|row| row.device_id == identity.device_id)
            .map(|row| row.state)
    };
    assert_eq!(sync_state(&state), Some(SyncState::Locked));

    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://127.0.0.1:47990/stream/audio.wav".into(),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone())
    .with_sync(state.sync.clone(), identity.clone());
    state
        .activate_sender_as(Box::new(sender), Some(identity.clone()))
        .await
        .unwrap();
    let plays = || {
        captured
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|action| action.contains("#Play"))
            .count()
    };
    let actions = || captured.0.lock().unwrap().len();
    // Activation: SetAVTransportURI + Play. Nothing pulls the stream, so
    // stall recovery supervises.
    assert_eq!((actions(), plays()), (2, 1));

    // The recovery notices the stall after its initial 2 s delay, re-enters
    // the group (the sync status drops to `syncing`) and holds the restart
    // until the group's epoch instead of firing immediately.
    tokio::time::sleep(Duration::from_millis(3000)).await;
    assert_eq!(
        actions(),
        2,
        "recovery must not restart playback before the group's epoch"
    );
    assert_eq!(
        sync_state(&state),
        Some(SyncState::Syncing),
        "re-entry reports syncing, not the stale locked report"
    );

    // Once the epoch passes, the restart goes through as SetAVTransportURI
    // + Play (SET_URI_SETTLE_DELAY included).
    let restarted = tokio::time::timeout(Duration::from_millis(4000), async {
        loop {
            if plays() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        restarted.is_ok(),
        "the recovery restart must still happen at the group's epoch"
    );
    assert!(
        Instant::now() >= epoch,
        "the restart fired before the epoch"
    );
    assert_eq!(sync_state(&state), Some(SyncState::Syncing));
}

/// A sync context alone changes nothing for a free-running output: with no
/// group to re-enter, recovery keeps its historical fire-now restart.
#[tokio::test]
async fn recovery_of_a_free_running_output_still_restarts_immediately() {
    use on_air_core::state::{ActiveOutput, CoreState};
    use std::time::Duration;

    let (addr, captured) = start_fake_sonos().await;
    let device = SonosDevice::discovered(
        "uuid:free-recovery",
        format!("http://{addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Free Recovery",
    );
    let identity = ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:free-recovery".into(),
        device_name: "Free Recovery".into(),
    };
    let mut state = CoreState::new();
    state.set_mock(true);
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://127.0.0.1:47990/stream/audio.wav".into(),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone())
    .with_sync(state.sync.clone(), identity.clone());
    state
        .activate_sender_as(Box::new(sender), Some(identity))
        .await
        .unwrap();
    let recovered = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let play_count = captured
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|action| action.contains("#Play"))
                .count();
            if play_count >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        recovered.is_ok(),
        "a free-running output must keep its fire-now recovery"
    );
}
