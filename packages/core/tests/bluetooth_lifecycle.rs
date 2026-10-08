use bytes::Bytes;
use on_air_core::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, BluetoothEndpoint, BluetoothPlaybackConfig, BluetoothSender,
    OpenedPcmSink, PcmOutput, PcmSink, RecordingPcmSink, SystemPcmOutput,
};
use on_air_core::sender::{AudioSender, OutputFormat, SenderError};
use on_air_core::state::CoreState;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, Notify};

type Log = Arc<Mutex<Vec<String>>>;
fn device() -> BluetoothDevice {
    BluetoothDevice {
        id: "speaker-id".into(),
        name: "JBL Bluetooth Speaker".into(),
        paired: true,
        connected: false,
        audio_endpoint: None,
    }
}
fn config(volume: u8) -> BluetoothPlaybackConfig {
    BluetoothPlaybackConfig {
        pipeline_hz: 44_100,
        output_hz: 44_100,
        volume,
    }
}
struct Adapter {
    log: Log,
    missing_endpoint: bool,
    lists: AtomicUsize,
}
impl BluetoothAdapter for Adapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        vec![device()]
    }
    fn pair(&self, _: &str) -> Result<(), SenderError> {
        Ok(())
    }
    fn connect(&self, id: &str) -> Result<BluetoothEndpoint, SenderError> {
        assert_eq!(id, "speaker-id");
        self.log.lock().unwrap().push("connect".into());
        if self.missing_endpoint {
            return Err(SenderError::not_ready("endpoint not ready"));
        }
        Ok(BluetoothEndpoint::Native("JBL Bluetooth Speaker".into()))
    }
    fn disconnect(&self, _: &str) -> Result<(), SenderError> {
        self.log.lock().unwrap().push("disconnect".into());
        Ok(())
    }
    fn open_settings(&self) -> Result<(), SenderError> {
        Ok(())
    }
}
struct Sink {
    log: Log,
    recording: RecordingPcmSink,
    entered: Notify,
    written: Notify,
    closed: Mutex<bool>,
    wake: Condvar,
    block: bool,
    reject_volume: AtomicBool,
}
impl PcmSink for Sink {
    fn write(&self, pcm: &[u8]) -> Result<(), SenderError> {
        self.entered.notify_one();
        let mut closed = self.closed.lock().unwrap();
        while self.block && !*closed {
            closed = self.wake.wait(closed).unwrap();
        }
        if !*closed {
            self.recording.write(pcm)?;
        }
        self.written.notify_one();
        Ok(())
    }
    fn set_volume(&self, volume: u8) -> Result<(), SenderError> {
        if self.reject_volume.load(Ordering::SeqCst) {
            return Err(SenderError::internal("volume rejected"));
        }
        self.log.lock().unwrap().push(format!("volume:{volume}"));
        self.recording.set_volume(volume)
    }
    fn close(&self) -> Result<(), SenderError> {
        let mut closed = self.closed.lock().unwrap();
        if !*closed {
            self.log.lock().unwrap().push("close".into());
        }
        *closed = true;
        self.wake.notify_all();
        Ok(())
    }
}
struct Output {
    log: Log,
    sinks: Mutex<Vec<Arc<Sink>>>,
    block: bool,
    fail: AtomicBool,
}
impl Output {
    fn latest(&self) -> Arc<Sink> {
        self.sinks.lock().unwrap().last().unwrap().clone()
    }
}
impl PcmOutput for Output {
    fn open(
        &self,
        endpoint: &BluetoothEndpoint,
        pipeline_hz: u32,
        preferred_hz: u32,
    ) -> Result<OpenedPcmSink, SenderError> {
        assert_eq!(
            endpoint,
            &BluetoothEndpoint::Native("JBL Bluetooth Speaker".into())
        );
        self.log
            .lock()
            .unwrap()
            .push(format!("open:{pipeline_hz}:{preferred_hz}"));
        if self.fail.load(Ordering::SeqCst) {
            return Err(SenderError::internal("open failed"));
        }
        let sink = Arc::new(Sink {
            log: self.log.clone(),
            recording: RecordingPcmSink::default(),
            entered: Notify::new(),
            written: Notify::new(),
            closed: Mutex::new(false),
            wake: Condvar::new(),
            block: self.block,
            reject_volume: AtomicBool::new(false),
        });
        self.sinks.lock().unwrap().push(sink.clone());
        Ok(OpenedPcmSink {
            sink,
            format: OutputFormat {
                sample_rate_hz: preferred_hz,
                supported_hz: vec![44_100, 48_000],
            },
        })
    }
}
struct Fixture {
    adapter: Arc<Adapter>,
    output: Arc<Output>,
    audio: broadcast::Sender<Bytes>,
    log: Log,
}
impl Fixture {
    fn new(block: bool, missing_endpoint: bool, fail_open: bool) -> Self {
        let log: Log = Arc::default();
        Self {
            adapter: Arc::new(Adapter {
                log: log.clone(),
                missing_endpoint,
                lists: AtomicUsize::new(0),
            }),
            output: Arc::new(Output {
                log: log.clone(),
                sinks: Mutex::new(Vec::new()),
                block,
                fail: AtomicBool::new(fail_open),
            }),
            audio: broadcast::channel(8).0,
            log,
        }
    }
    fn sender(&self, settings: BluetoothPlaybackConfig) -> BluetoothSender {
        BluetoothSender::new(
            device(),
            self.adapter.clone(),
            self.audio.clone(),
            self.output.clone(),
            settings,
        )
    }
    async fn sample(&self, value: i16) -> i16 {
        let sink = self.output.latest();
        self.audio
            .send(Bytes::copy_from_slice(&value.to_le_bytes()))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), sink.written.notified())
            .await
            .unwrap();
        let chunks = sink.recording.chunks.lock();
        i16::from_le_bytes(chunks.last().unwrap()[..2].try_into().unwrap())
    }
}

#[tokio::test]
async fn one_connect_resolves_endpoint_before_open_and_volume_never_rediscovers() {
    let f = Fixture::new(false, false, false);
    let mut sender = f.sender(config(37));
    assert!(
        f.log.lock().unwrap().is_empty(),
        "construction must not start playback"
    );
    sender.start().await.unwrap();
    assert_eq!(
        *f.log.lock().unwrap(),
        ["connect", "open:44100:44100", "volume:37"]
    );
    assert_eq!(f.sample(10_000).await, 3_700);
    sender.set_volume(0).await.unwrap();
    assert_eq!(f.sample(-10_000).await, 0);
    sender.set_volume(100).await.unwrap();
    assert_eq!(f.sample(i16::MIN).await, i16::MIN);
    assert_eq!(f.adapter.lists.load(Ordering::SeqCst), 0);
    sender.stop().await.unwrap();
}

#[tokio::test]
async fn stop_reopens_a_fresh_sink_and_retains_volume() {
    let f = Fixture::new(false, false, false);
    let mut sender = f.sender(config(100));
    sender.start().await.unwrap();
    sender.set_volume(37).await.unwrap();
    assert_eq!(f.sample(10_000).await, 3_700);
    let first = f.output.latest();
    sender.stop().await.unwrap();
    assert!(*first.closed.lock().unwrap());
    sender.stop().await.unwrap();
    sender.start().await.unwrap();
    assert!(!Arc::ptr_eq(&first, &f.output.latest()));
    assert_eq!(f.sample(20_000).await, 7_400);
    assert_eq!(first.recording.chunks.lock().len(), 1);
    assert_eq!(f.output.sinks.lock().unwrap().len(), 2);
    sender.stop().await.unwrap();
}

#[tokio::test]
async fn stop_and_drop_cancel_a_blocked_write() {
    for drop_sender in [false, true] {
        let f = Fixture::new(true, false, false);
        let mut sender = f.sender(config(100));
        sender.start().await.unwrap();
        let sink = f.output.latest();
        f.audio.send(Bytes::from_static(&[0, 1])).unwrap();
        tokio::time::timeout(Duration::from_secs(1), sink.entered.notified())
            .await
            .unwrap();
        if drop_sender {
            drop(sender);
        } else {
            tokio::time::timeout(Duration::from_secs(1), sender.stop())
                .await
                .unwrap()
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(1), sink.written.notified())
            .await
            .unwrap();
        assert!(*sink.closed.lock().unwrap());
    }
}

#[tokio::test]
async fn missing_endpoint_and_open_failure_leave_no_live_playback() {
    for (missing, fail) in [(true, false), (false, true)] {
        let f = Fixture::new(false, missing, fail);
        let mut sender = f.sender(config(50));
        assert!(sender.start().await.is_err());
        sender.stop().await.unwrap();
        assert!(sender.output_format().is_none());
        assert!(f.output.sinks.lock().unwrap().is_empty());
        assert_eq!(f.log.lock().unwrap().last().unwrap(), "disconnect");
        if missing {
            assert!(!f
                .log
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry.starts_with("open:")));
        }
    }
}

struct Replacement {
    log: Log,
    fail: bool,
    fail_stop: bool,
}
#[async_trait::async_trait]
impl AudioSender for Replacement {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.log.lock().unwrap().push("replacement:start".into());
        if self.fail {
            Err(SenderError::transport("replacement failed"))
        } else {
            Ok(())
        }
    }
    async fn stop(&mut self) -> Result<(), SenderError> {
        self.log.lock().unwrap().push("replacement:stop".into());
        if self.fail_stop {
            Err(SenderError::transport("replacement cleanup failed"))
        } else {
            Ok(())
        }
    }
    async fn set_volume(&mut self, _: u8) -> Result<(), SenderError> {
        Ok(())
    }
    fn name(&self) -> &str {
        "replacement"
    }
    fn transport(&self) -> &'static str {
        "test"
    }
}

#[tokio::test]
async fn failed_switch_reopens_bluetooth_with_saved_volume_and_format() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(f.sender(config(37))))
        .await
        .unwrap();
    assert!(state
        .activate_sender(Box::new(Replacement {
            log: f.log.clone(),
            fail: true,
            fail_stop: false
        }))
        .await
        .is_err());
    assert_eq!(
        *f.log.lock().unwrap(),
        [
            "connect",
            "open:44100:44100",
            "volume:37",
            "close",
            "disconnect",
            "replacement:start",
            "replacement:stop",
            "connect",
            "open:44100:44100",
            "volume:37"
        ]
    );
    assert_eq!(f.sample(10_000).await, 3_700);
    assert_eq!(*state.output_sample_rate_hz.lock(), 44_100);
    state.deactivate_sender().await.unwrap();
}

#[tokio::test]
async fn failed_cleanup_retains_new_sender_and_does_not_resume_bluetooth() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(f.sender(config(37))))
        .await
        .unwrap();
    let error = state
        .activate_sender(Box::new(Replacement {
            log: f.log.clone(),
            fail: true,
            fail_stop: true,
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(error, SenderError::RollbackFailed { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("cleanup after failed start"));
    assert_eq!(
        f.output.sinks.lock().unwrap().len(),
        1,
        "old output must not reopen"
    );
    assert_eq!(
        state.active_sender.lock().await.as_ref().unwrap().name(),
        "replacement"
    );
    assert!(
        state.deactivate_sender().await.is_err(),
        "unresolved sender remains available for cleanup"
    );
}

#[tokio::test]
async fn failed_previous_stop_prevents_bluetooth_connection_and_open() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(Replacement {
            log: f.log.clone(),
            fail: false,
            fail_stop: true,
        }))
        .await
        .unwrap();
    assert!(state
        .activate_sender(Box::new(f.sender(config(50))))
        .await
        .is_err());
    assert_eq!(
        *f.log.lock().unwrap(),
        ["replacement:start", "replacement:stop"]
    );
    assert!(f.output.sinks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn changing_rates_closes_old_stream_before_opening_the_new_one() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(f.sender(config(50))))
        .await
        .unwrap();
    let mut changed = config(50);
    changed.pipeline_hz = 96_000;
    changed.output_hz = 48_000;
    state
        .activate_sender(Box::new(f.sender(changed)))
        .await
        .unwrap();
    assert_eq!(
        *f.log.lock().unwrap(),
        [
            "connect",
            "open:44100:44100",
            "volume:50",
            "close",
            "disconnect",
            "connect",
            "open:96000:48000",
            "volume:50"
        ]
    );
    assert_eq!(*state.output_sample_rate_hz.lock(), 48_000);
    let format = state
        .active_sender
        .lock()
        .await
        .as_ref()
        .unwrap()
        .output_format()
        .unwrap();
    assert_eq!(format.supported_hz, [44_100, 48_000]);
    state.deactivate_sender().await.unwrap();
}

#[tokio::test]
async fn failed_volume_write_keeps_saved_volume_and_rollback_volume() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(f.sender(config(37))))
        .await
        .unwrap();
    on_air_core::session::set_volume(&state, 37).await.unwrap();
    f.output
        .latest()
        .reject_volume
        .store(true, Ordering::SeqCst);
    assert!(on_air_core::session::set_volume(&state, 0).await.is_err());
    assert_eq!(state.output_volume.load(Ordering::SeqCst), 37);
    assert!(state
        .activate_sender(Box::new(Replacement {
            log: f.log.clone(),
            fail: true,
            fail_stop: false
        }))
        .await
        .is_err());
    assert_eq!(f.sample(10_000).await, 3_700);
    state.deactivate_sender().await.unwrap();
}

#[test]
fn native_bluetooth_name_does_not_select_pacat() {
    let result = SystemPcmOutput.open(
        &BluetoothEndpoint::Native("Bluetooth nonexistent lifecycle test endpoint".into()),
        44_100,
        44_100,
    );
    let error = match result {
        Ok(_) => panic!("nonexistent endpoint opened"),
        Err(error) => error,
    };
    assert!(matches!(error, SenderError::DeviceNotFound(_)), "{error}");
}

struct GatedOutput {
    output: Arc<Output>,
    entered: Notify,
    released: Mutex<bool>,
    wake: Condvar,
}
impl GatedOutput {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}
impl PcmOutput for GatedOutput {
    fn open(
        &self,
        endpoint: &BluetoothEndpoint,
        pipeline: u32,
        preferred: u32,
    ) -> Result<OpenedPcmSink, SenderError> {
        let opened = self.output.open(endpoint, pipeline, preferred)?;
        self.entered.notify_one();
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.wake.wait(released).unwrap();
        }
        Ok(opened)
    }
}

#[tokio::test]
async fn cancellation_closes_preparation_even_when_the_factory_retains_the_sink() {
    let f = Fixture::new(false, false, false);
    let gate = Arc::new(GatedOutput {
        output: f.output.clone(),
        entered: Notify::new(),
        released: Mutex::new(false),
        wake: Condvar::new(),
    });
    let mut sender = BluetoothSender::new(
        device(),
        f.adapter.clone(),
        f.audio.clone(),
        gate.clone(),
        config(50),
    );
    let start = tokio::spawn(async move { sender.start().await });
    tokio::time::timeout(Duration::from_secs(1), gate.entered.notified())
        .await
        .unwrap();
    start.abort();
    assert!(start.await.unwrap_err().is_cancelled());
    gate.release();
    let sink = f.output.latest();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !*sink.closed.lock().unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn sample_rate_polling_does_not_wait_for_output_preparation() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    let f = Fixture::new(false, false, false);
    let gate = Arc::new(GatedOutput {
        output: f.output.clone(),
        entered: Notify::new(),
        released: Mutex::new(false),
        wake: Condvar::new(),
    });
    let state = CoreState::new_mock().await;
    let mut settings = config(50);
    settings.output_hz = 48_000;
    let sender = BluetoothSender::new(
        device(),
        f.adapter.clone(),
        f.audio.clone(),
        gate.clone(),
        settings,
    );
    let activating = state.clone();
    let start = tokio::spawn(async move { activating.activate_sender(Box::new(sender)).await });
    tokio::time::timeout(Duration::from_secs(1), gate.entered.notified())
        .await
        .unwrap();
    let app = on_air_core::build_router(state.clone());
    let polling = tokio::time::timeout(
        Duration::from_millis(200),
        app.clone().oneshot(
            Request::builder()
                .uri("/api/sample-rate")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await;
    gate.release(); // Release even on failure, so the regression test cannot strand a worker.
    start.await.unwrap().unwrap();
    assert_eq!(
        polling
            .expect("polling blocked behind preparation")
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sample-rate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rates: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rates["output"]["sample_rate_hz"], 48_000);
    assert_eq!(
        rates["output"]["supported_hz"],
        serde_json::json!([44_100, 48_000])
    );
    state.deactivate_sender().await.unwrap();
}

#[tokio::test]
async fn failed_bluetooth_rollback_cleans_up_its_connection_attempt() {
    let f = Fixture::new(false, false, false);
    let state = CoreState::new_mock().await;
    state
        .activate_sender(Box::new(f.sender(config(37))))
        .await
        .unwrap();
    f.output.fail.store(true, Ordering::SeqCst);
    let error = state
        .activate_sender(Box::new(Replacement {
            log: f.log.clone(),
            fail: true,
            fail_stop: false,
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(error, SenderError::RollbackFailed { .. }),
        "{error}"
    );
    assert!(error
        .to_string()
        .contains("restoring previous output failed"));
    assert!(state.active_sender.lock().await.is_none());
    assert!(state.active_output.lock().is_none());
    assert_eq!(f.log.lock().unwrap().last().unwrap(), "disconnect");
    assert!(*f.output.latest().closed.lock().unwrap());
}
