use on_air_core::sender::{AudioSender, NullSender};
use on_air_core::state::CoreState;
use std::sync::Arc;
use tokio::sync::Mutex;

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
