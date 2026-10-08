use on_air_core::api::ws::WsEvent;
use on_air_core::sender::{AudioSender, NullSender, SenderError};
use on_air_core::state::CoreState;
use std::sync::Arc;
use tokio::sync::Mutex;

struct FailingStopSender {
    name: String,
    log: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl AudioSender for FailingStopSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:start", self.name));
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:stop", self.name));
        Err(SenderError::transport("stop failed"))
    }

    async fn set_volume(&mut self, _volume: u8) -> Result<(), SenderError> {
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn transport(&self) -> &'static str {
        "null"
    }
}

#[tokio::test]
async fn activating_new_sender_stops_previous_first() {
    let state = CoreState::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    state
        .activate_sender(Box::new(NullSender::new("A", log.clone())))
        .await
        .unwrap();
    state
        .activate_sender(Box::new(NullSender::new("B", log.clone())))
        .await
        .unwrap();

    let entries = log.lock().await.clone();
    assert_eq!(entries, vec!["A:start", "A:stop", "B:start"]);
}

#[tokio::test]
async fn activate_ws_event_uses_the_sender_transport() {
    let state = CoreState::new();
    let mut rx = state.ws_tx.subscribe();
    let log = Arc::new(Mutex::new(Vec::new()));
    state
        .activate_sender(Box::new(NullSender::new("BT", log)))
        .await
        .unwrap();
    match rx.try_recv() {
        Ok(WsEvent::OutputStateChanged {
            transport, active, ..
        }) => {
            assert_eq!(transport, "null");
            assert!(active);
        }
        other => panic!("expected output_state_changed, got {other:?}"),
    }
}

#[tokio::test]
async fn deactivate_stops_the_active_sender() {
    let state = CoreState::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    state
        .activate_sender(Box::new(NullSender::new("A", log.clone())))
        .await
        .unwrap();
    state.deactivate_sender().await.unwrap();

    let entries = log.lock().await.clone();
    assert_eq!(entries, vec!["A:start", "A:stop"]);
}

#[tokio::test]
async fn deactivate_with_no_active_sender_is_a_no_op() {
    let state = CoreState::new();
    state.deactivate_sender().await.unwrap();
}

#[tokio::test]
async fn failed_stop_blocks_activating_the_next_sender_and_preserves_the_active_sender() {
    let state = CoreState::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    state
        .activate_sender(Box::new(FailingStopSender {
            name: "A".into(),
            log: log.clone(),
        }))
        .await
        .unwrap();
    let error = state
        .activate_sender(Box::new(NullSender::new("B", log.clone())))
        .await
        .unwrap_err();

    let entries = log.lock().await.clone();
    assert_eq!(entries, vec!["A:start", "A:stop"]);
    assert!(matches!(error, SenderError::StopFailed(_)), "{error}");
    assert!(state.output().sender_name().await.is_some());
}

#[tokio::test]
async fn failed_deactivation_preserves_the_active_sender() {
    let state = CoreState::new();
    let log = Arc::new(Mutex::new(Vec::new()));

    state
        .activate_sender(Box::new(FailingStopSender {
            name: "A".into(),
            log,
        }))
        .await
        .unwrap();

    assert!(state.deactivate_sender().await.is_err());
    assert!(state.output().sender_name().await.is_some());
}
