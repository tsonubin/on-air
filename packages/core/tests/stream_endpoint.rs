use futures_util::StreamExt;
use on_air_core::state::{ActiveOutput, CoreState};
use std::sync::atomic::Ordering;
use tokio::net::TcpListener;

#[tokio::test]
async fn streams_published_pcm_chunks_with_correct_content_type() {
    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
    let audio_tx = state.audio_tx.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // give the server a moment to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let known_chunk = bytes::Bytes::from_static(&[1, 2, 3, 4]);
    let audio_tx_clone = audio_tx.clone();
    let known_chunk_clone = known_chunk.clone();
    tokio::spawn(async move {
        // keep publishing until a subscriber (the HTTP request below) picks one up
        loop {
            let _ = audio_tx_clone.send(known_chunk_clone.clone());
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });

    let response = reqwest::get(format!("http://{addr}/stream/audio.wav"))
        .await
        .unwrap();
    assert_eq!(response.headers().get("content-type").unwrap(), "audio/wav");

    let mut stream = response.bytes_stream();
    let first_chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("received a chunk before timing out")
        .expect("stream not closed")
        .expect("chunk read ok");
    assert!(
        first_chunk.starts_with(b"RIFF"),
        "wav header first: {first_chunk:?}"
    );

    let pcm_chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("received pcm before timing out")
        .expect("stream not closed")
        .expect("chunk read ok");
    assert_eq!(pcm_chunk, known_chunk);
}

#[tokio::test]
async fn bursty_sonos_reader_keeps_buffered_audio_contiguous() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
    let audio_tx = state.audio_tx.clone();
    let response = on_air_core::build_router(state)
        .oneshot(
            Request::builder()
                .uri("/stream/audio.wav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();

    // Sonos radio clients read in bursts while their playback buffer drains.
    // Queue roughly 2.3 seconds of 1024-frame chunks before polling the body.
    for sequence in 0u16..100 {
        audio_tx
            .send(bytes::Bytes::copy_from_slice(&sequence.to_le_bytes()))
            .unwrap();
    }

    let header = stream.next().await.unwrap().unwrap();
    assert!(header.starts_with(b"RIFF"));
    let first_pcm = stream.next().await.unwrap().unwrap();
    assert_eq!(
        first_pcm,
        bytes::Bytes::copy_from_slice(&0u16.to_le_bytes())
    );
}

#[tokio::test]
async fn stream_is_absent_unless_sonos_is_the_exclusive_output() {
    let state = CoreState::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let response = reqwest::get(format!("http://{addr}/stream/audio.wav"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_is_unavailable_while_service_is_disabled() {
    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
    state.service_enabled.store(false, Ordering::Release);
    let app = on_air_core::build_router(state);
    let response = tower::ServiceExt::oneshot(
        app,
        axum::http::Request::builder()
            .uri("/stream/audio.wav")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn silent_stream_closes_when_service_is_disabled() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
    let response = on_air_core::build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/stream/audio.wav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();
    assert!(stream.next().await.unwrap().unwrap().starts_with(b"RIFF"));
    assert!(stream
        .next()
        .await
        .unwrap()
        .unwrap()
        .iter()
        .all(|byte| *byte == 0));
    assert_eq!(state.stream_clients.load(Ordering::Acquire), 1);

    state.set_service_enabled(false);
    tokio::time::timeout(std::time::Duration::from_millis(250), async {
        while stream.next().await.is_some() {}
    })
    .await
    .expect("silent HTTP body stayed live after the service was disabled");
    drop(stream);
    assert_eq!(state.stream_clients.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn silent_stream_releases_its_slot_when_output_is_deactivated() {
    use axum::body::Body;
    use axum::http::Request;
    use on_air_core::sender::NullSender;
    use tower::ServiceExt;

    let state = CoreState::new();
    state
        .activate_sender_as(
            Box::new(NullSender::new("Test", state.mock_log.clone())),
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:test".into(),
                device_name: "Test".into(),
            }),
        )
        .await
        .unwrap();
    let response = on_air_core::build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/stream/audio.wav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();
    assert!(stream.next().await.unwrap().unwrap().starts_with(b"RIFF"));
    assert!(stream
        .next()
        .await
        .unwrap()
        .unwrap()
        .iter()
        .all(|byte| *byte == 0));
    assert_eq!(state.stream_clients.load(Ordering::Acquire), 1);

    state.deactivate_sender().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_millis(250), async {
        while stream.next().await.is_some() {}
    })
    .await
    .expect("silent HTTP body ignored the output generation change");
    drop(stream);
    assert_eq!(state.stream_clients.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn stream_is_live_when_airplay_is_the_exclusive_output() {
    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "airplay".into(),
        device_id: "EE:C7:74:A7:D8:56".into(),
        device_name: "卧室".into(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let response = reqwest::get(format!("http://{addr}/stream/audio.wav"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn sonos_radio_is_live_before_play_pulls_the_uri() {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::Router;
    use on_air_core::sender::sonos::discovery::SonosDevice;
    use on_air_core::sender::sonos::SonosSender;
    use on_air_core::sender::AudioSender;
    use on_air_core::state::ActiveOutput;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Captured {
        uri: Arc<Mutex<Option<String>>>,
        play_status: Arc<Mutex<Option<u16>>>,
    }

    async fn soap(State(cap): State<Captured>, headers: HeaderMap, body: String) -> &'static str {
        let action = headers
            .get("soapaction")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if action.contains("SetAVTransportURI") {
            if let Some(rest) = body.split("<CurrentURI>").nth(1) {
                if let Some(uri) = rest.split("</CurrentURI>").next() {
                    if uri.starts_with("http://") {
                        *cap.uri.lock().unwrap() = Some(uri.to_string());
                    }
                }
            }
        }
        if action.contains("#Play") {
            let uri = cap.uri.lock().unwrap().clone();
            let status_slot = cap.play_status.clone();
            tokio::spawn(async move {
                if let Some(uri) = uri {
                    let client = reqwest::Client::builder()
                        .no_proxy()
                        .timeout(std::time::Duration::from_secs(2))
                        .build()
                        .unwrap();
                    if let Ok(resp) = client.get(uri).send().await {
                        *status_slot.lock().unwrap() = Some(resp.status().as_u16());
                    }
                }
            });
        }
        "OK"
    }

    let cap = Captured::default();
    let fake = Router::new()
        .route("/MediaRenderer/AVTransport/Control", post(soap))
        .route("/MediaRenderer/RenderingControl/Control", post(soap))
        .with_state(cap.clone());
    let fake_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake_addr = fake_l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(fake_l, fake).await.unwrap();
    });

    let state = CoreState::new();
    let core_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core_addr = core_l.local_addr().unwrap();
    let app = on_air_core::build_router(state.clone());
    tokio::spawn(async move {
        axum::serve(core_l, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let device = SonosDevice::discovered(
        "uuid:fake",
        format!("http://{fake_addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Fake",
    );
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        format!("http://{core_addr}/stream/audio.wav"),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone());
    state
        .activate_sender_as(
            Box::new(sender) as Box<dyn AudioSender>,
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:fake".into(),
                device_name: "Fake".into(),
            }),
        )
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let status = loop {
        if let Some(code) = *cap.play_status.lock().unwrap() {
            break Some(code);
        }
        if tokio::time::Instant::now() >= deadline {
            break None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(status, Some(200), "speaker GET during Play must not 404");
}

#[derive(Clone, Copy)]
enum FakeSonosFailure {
    InactivityTimeout,
    DropFirstPull,
    StallFirstPull,
}

#[derive(Clone)]
struct SonosRecoveryProbe {
    inner: std::sync::Arc<SonosRecoveryProbeInner>,
}

struct SonosRecoveryProbeInner {
    failure: FakeSonosFailure,
    uri: std::sync::Mutex<Option<String>>,
    play_count: std::sync::atomic::AtomicUsize,
    pull_count: std::sync::atomic::AtomicUsize,
    first_audio_seen: std::sync::atomic::AtomicBool,
    resumed_audio_seen: std::sync::atomic::AtomicBool,
}

impl SonosRecoveryProbe {
    fn new(failure: FakeSonosFailure) -> Self {
        Self {
            inner: std::sync::Arc::new(SonosRecoveryProbeInner {
                failure,
                uri: std::sync::Mutex::new(None),
                play_count: std::sync::atomic::AtomicUsize::new(0),
                pull_count: std::sync::atomic::AtomicUsize::new(0),
                first_audio_seen: std::sync::atomic::AtomicBool::new(false),
                resumed_audio_seen: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }
}

const FIRST_AUDIO: &[u8] = &[0x11, 0x22, 0x33, 0x44];
const RESUMED_AUDIO: &[u8] = &[0x55, 0x66, 0x77, 0x7f];

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

async fn fake_sonos_pull(uri: String, probe: SonosRecoveryProbe, pull_number: usize) {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let Ok(response) = client.get(uri).send().await else {
        return;
    };
    probe
        .inner
        .pull_count
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let mut body = response.bytes_stream();
    loop {
        let item = tokio::time::timeout(std::time::Duration::from_millis(120), body.next()).await;
        let bytes = match item {
            Ok(Some(Ok(bytes))) => bytes,
            _ => return,
        };
        if contains_bytes(&bytes, FIRST_AUDIO) {
            probe
                .inner
                .first_audio_seen
                .store(true, std::sync::atomic::Ordering::Release);
            if matches!(probe.inner.failure, FakeSonosFailure::DropFirstPull) && pull_number == 1 {
                return;
            }
        }
        if contains_bytes(&bytes, RESUMED_AUDIO) {
            probe
                .inner
                .resumed_audio_seen
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

async fn recovery_soap(
    axum::extract::State(probe): axum::extract::State<SonosRecoveryProbe>,
    headers: axum::http::HeaderMap,
    body: String,
) -> String {
    let action = headers
        .get("soapaction")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if action.contains("SetAVTransportURI") {
        if let Some(uri) = body
            .split("<CurrentURI>")
            .nth(1)
            .and_then(|rest| rest.split("</CurrentURI>").next())
        {
            *probe.inner.uri.lock().unwrap() = Some(uri.to_string());
        }
    } else if action.contains("#Play") {
        let pull_number = probe
            .inner
            .play_count
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        let simulate_existing_stalled_pull =
            matches!(probe.inner.failure, FakeSonosFailure::StallFirstPull) && pull_number == 1;
        if !simulate_existing_stalled_pull {
            let uri = probe.inner.uri.lock().unwrap().clone();
            if let Some(uri) = uri {
                let pull_probe = probe.clone();
                tokio::spawn(fake_sonos_pull(uri, pull_probe, pull_number));
            }
        }
    } else if action.contains("GetTransportInfo") {
        // Model the reported unhealthy state: Sonos still says PLAYING even
        // after its HTTP pull has disappeared.
        return "<CurrentTransportState>PLAYING</CurrentTransportState>".into();
    }
    "OK".into()
}

async fn wait_until(timeout: std::time::Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if predicate() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    predicate()
}

async fn start_sonos_recovery_harness(
    failure: FakeSonosFailure,
) -> (
    CoreState,
    tokio::sync::broadcast::Sender<bytes::Bytes>,
    SonosRecoveryProbe,
    Option<tokio::sync::oneshot::Sender<()>>,
) {
    use axum::routing::post;
    use axum::Router;
    use on_air_core::sender::sonos::discovery::SonosDevice;
    use on_air_core::sender::sonos::SonosSender;
    use on_air_core::sender::AudioSender;
    use std::net::{IpAddr, Ipv4Addr};

    let probe = SonosRecoveryProbe::new(failure);
    let fake = Router::new()
        .route("/MediaRenderer/AVTransport/Control", post(recovery_soap))
        .route(
            "/MediaRenderer/RenderingControl/Control",
            post(recovery_soap),
        )
        .with_state(probe.clone());
    let fake_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fake_addr = fake_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(fake_listener, fake).await.unwrap();
    });

    let state = CoreState::new();
    let audio_tx = state.audio_tx.clone();
    let core_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core_addr = core_listener.local_addr().unwrap();
    let core = on_air_core::build_router(state.clone());
    tokio::spawn(async move {
        axum::serve(core_listener, core).await.unwrap();
    });

    let stalled_reader = if matches!(failure, FakeSonosFailure::StallFirstPull) {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        *state.active_output.lock().unwrap() = Some(ActiveOutput {
            transport: "sonos".into(),
            device_id: "uuid:recovery-test".into(),
            device_name: "Recovery Test".into(),
        });
        let response = on_air_core::build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/stream/audio.wav")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut stream = response.into_body().into_data_stream();
        assert!(stream.next().await.unwrap().unwrap().starts_with(b"RIFF"));
        audio_tx
            .send(bytes::Bytes::from_static(FIRST_AUDIO))
            .unwrap();
        let pcm = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("stalled reader did not receive initial PCM")
            .expect("stalled reader closed before initial PCM")
            .expect("stalled reader failed before initial PCM");
        assert!(contains_bytes(&pcm, FIRST_AUDIO));
        probe
            .inner
            .pull_count
            .store(1, std::sync::atomic::Ordering::Release);
        probe
            .inner
            .first_audio_seen
            .store(true, std::sync::atomic::Ordering::Release);
        let (release, hold) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = hold.await;
            drop(stream);
        });
        Some(release)
    } else {
        None
    };

    let device = SonosDevice::discovered(
        "uuid:recovery-test",
        format!("http://{fake_addr}/xml/device_description.xml"),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Recovery Test",
    );
    let sender = SonosSender::new(
        device,
        reqwest::Client::builder().no_proxy().build().unwrap(),
        format!("http://{core_addr}/stream/audio.wav"),
    )
    .with_stream_health(state.stream_clients.clone(), state.stream_progress.clone());
    state
        .activate_sender_as(
            Box::new(sender) as Box<dyn AudioSender>,
            Some(ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:recovery-test".into(),
                device_name: "Recovery Test".into(),
            }),
        )
        .await
        .unwrap();
    assert!(
        wait_until(std::time::Duration::from_secs(1), || {
            probe
                .inner
                .pull_count
                .load(std::sync::atomic::Ordering::Acquire)
                >= 1
        })
        .await,
        "fake Sonos never pulled the live stream"
    );
    (state, audio_tx, probe, stalled_reader)
}

async fn publish_until_seen(
    audio_tx: &tokio::sync::broadcast::Sender<bytes::Bytes>,
    pcm: &'static [u8],
    seen: &std::sync::atomic::AtomicBool,
) -> bool {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(6_500);
    while tokio::time::Instant::now() < deadline {
        let _ = audio_tx.send(bytes::Bytes::from_static(pcm));
        if seen.load(std::sync::atomic::Ordering::Acquire) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    seen.load(std::sync::atomic::Ordering::Acquire)
}

#[tokio::test]
async fn sonos_stream_resumes_after_input_stops_producing_pcm() {
    let (state, audio_tx, probe, _stalled_reader) =
        start_sonos_recovery_harness(FakeSonosFailure::InactivityTimeout).await;
    assert!(
        publish_until_seen(&audio_tx, FIRST_AUDIO, &probe.inner.first_audio_seen).await,
        "fake Sonos never received initial PCM"
    );

    // A record change can leave capture producing no frames at all. This is
    // deliberately longer than the fake receiver's playback-buffer timeout.
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    assert!(state.active_output.lock().unwrap().is_some());

    assert!(
        publish_until_seen(&audio_tx, RESUMED_AUDIO, &probe.inner.resumed_audio_seen).await,
        "Sonos did not receive PCM again after input resumed"
    );
    assert!(
        probe
            .inner
            .play_count
            .load(std::sync::atomic::Ordering::Acquire)
            <= 3,
        "recovery must not use a busy Play retry loop"
    );
}

#[tokio::test]
async fn sonos_stream_recovers_when_reader_drops_but_output_remains_active() {
    let (state, audio_tx, probe, _stalled_reader) =
        start_sonos_recovery_harness(FakeSonosFailure::DropFirstPull).await;
    assert!(
        publish_until_seen(&audio_tx, FIRST_AUDIO, &probe.inner.first_audio_seen).await,
        "fake Sonos never received initial PCM"
    );
    assert!(state.active_output.lock().unwrap().is_some());

    assert!(
        publish_until_seen(&audio_tx, RESUMED_AUDIO, &probe.inner.resumed_audio_seen).await,
        "Sonos did not recover its HTTP pull without output reselection (plays={}, pulls={}, clients={})",
        probe
            .inner
            .play_count
            .load(std::sync::atomic::Ordering::Acquire),
        probe
            .inner
            .pull_count
            .load(std::sync::atomic::Ordering::Acquire),
        state.stream_clients.load(Ordering::Acquire),
    );
    assert!(
        probe
            .inner
            .play_count
            .load(std::sync::atomic::Ordering::Acquire)
            <= 3,
        "recovery must be bounded instead of replaying in a busy loop"
    );
}

#[tokio::test]
async fn sonos_stream_recovers_when_connected_reader_stalls() {
    let (state, audio_tx, probe, stalled_reader) =
        start_sonos_recovery_harness(FakeSonosFailure::StallFirstPull).await;
    let _stalled_reader = stalled_reader.expect("harness must retain the stalled stream body");
    assert_eq!(state.stream_clients.load(Ordering::Acquire), 1);
    assert!(state.active_output.lock().unwrap().is_some());

    assert!(
        publish_until_seen(&audio_tx, RESUMED_AUDIO, &probe.inner.resumed_audio_seen).await,
        "Sonos did not replace its connected but stalled HTTP pull (plays={}, pulls={}, clients={}, progress={})",
        probe
            .inner
            .play_count
            .load(std::sync::atomic::Ordering::Acquire),
        probe
            .inner
            .pull_count
            .load(std::sync::atomic::Ordering::Acquire),
        state.stream_clients.load(Ordering::Acquire),
        state.stream_progress.load(Ordering::Relaxed),
    );
    assert!(
        probe
            .inner
            .play_count
            .load(std::sync::atomic::Ordering::Acquire)
            <= 3,
        "stalled-reader recovery must back off instead of replaying in a busy loop"
    );
}

#[tokio::test]
async fn sonos_stream_reader_recovers_after_broadcast_lag() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let state = CoreState::new();
    *state.active_output.lock().unwrap() = Some(ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:test".into(),
        device_name: "Test".into(),
    });
    let audio_tx = state.audio_tx.clone();
    let response = on_air_core::build_router(state)
        .oneshot(
            Request::builder()
                .uri("/stream/audio.wav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();
    assert!(stream.next().await.unwrap().unwrap().starts_with(b"RIFF"));

    for sequence in 0u16..1000 {
        audio_tx
            .send(bytes::Bytes::copy_from_slice(&sequence.to_le_bytes()))
            .unwrap();
    }
    // The next poll observes Lagged internally, skips stale data, and must
    // advance to the retained tail rather than ending the HTTP body.
    let recovered = tokio::time::timeout(std::time::Duration::from_millis(300), stream.next())
        .await
        .expect("lagged stream stalled instead of advancing")
        .expect("lagged stream closed")
        .expect("lagged stream read failed");
    assert!(u16::from_le_bytes([recovered[0], recovered[1]]) > 0);
}
