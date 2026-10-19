//! The output invariant, pinned.
//!
//! OLD, now retired: one playing output per process ("process-wide
//! single-output"). Every assertion below that reads like it enforces that
//! rule is really enforcing its replacement at the level where it still
//! holds: ONE `OutputSession` never runs two senders — a second activation
//! in the same session tears the first one down, and a failed stop blocks
//! the switch.
//!
//! NEW: at most one live sender per session, and no two sessions may drive
//! the same receiver (a hard refusal — "This speaker is in use by another
//! on-air session"; pinned in `api_patchbay.rs`). Simultaneous multi-output
//! is first-class through sync groups of sessions (`on_air_core::sync`),
//! whose members start together at one presentation epoch. A sync group is
//! MORE constrained than two unrelated outputs, not less: outside a group
//! outputs free-run and drift apart, which is expected; inside a group they
//! are held to the shared epoch, the delay lines and drift steering.

use on_air_core::api::ws::WsEvent;
use on_air_core::sender::{AudioSender, NullSender, SenderError};
use on_air_core::session::OutputSession;
use on_air_core::state::{ActiveOutput, CoreState};
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

/// The retired rule allowed only one playing output per process. The
/// replacement allows several sessions live at once — the first-class
/// multi-output mode (sync groups of sessions) at its most basic.
#[tokio::test]
async fn two_sessions_play_at_the_same_time() {
    let (ws_tx, _) = tokio::sync::broadcast::channel(16);
    let session_a = OutputSession::new(ws_tx.clone(), Arc::new(parking_lot::Mutex::new(44_100)));
    let session_b = OutputSession::new(ws_tx, Arc::new(parking_lot::Mutex::new(44_100)));
    session_a.set_manage_local_sink(false);
    session_b.set_manage_local_sink(false);

    let log = Arc::new(Mutex::new(Vec::new()));
    session_a
        .activate(Box::new(NullSender::new("A", log.clone())), None, None)
        .await
        .unwrap();
    session_b
        .activate(Box::new(NullSender::new("B", log.clone())), None, None)
        .await
        .unwrap();

    assert_eq!(
        log.lock().await.as_slice(),
        ["A:start", "B:start"],
        "neither session stopped the other: multi-output is legal now"
    );
    assert_eq!(session_a.sender_name().await.as_deref(), Some("A"));
    assert_eq!(session_b.sender_name().await.as_deref(), Some("B"));
    session_a.deactivate().await.unwrap();
    session_b.deactivate().await.unwrap();
}

/// Sync groups relax the process-wide rule, never the per-session one:
/// inside a group a session still tears its previous sender down first.
#[tokio::test]
async fn one_session_still_stops_the_previous_sender_even_in_a_sync_group() {
    let state = CoreState::new();
    let group = state.sync.create_group("Together");
    state
        .sync
        .join(
            &group,
            &ActiveOutput {
                transport: "null".into(),
                device_id: "a".into(),
                device_name: "A".into(),
            },
        )
        .unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));

    state
        .activate_sender(Box::new(NullSender::new("A", log.clone())))
        .await
        .unwrap();
    state
        .activate_sender(Box::new(NullSender::new("B", log.clone())))
        .await
        .unwrap();

    assert_eq!(
        log.lock().await.as_slice(),
        ["A:start", "A:stop", "B:start"],
        "group membership must not weaken the session's own exclusivity"
    );
}

/// Group lifecycle, pinned at the registry: an output plays in at most one
/// group at a time — the second join is REFUSED, never a silent move.
#[tokio::test]
async fn an_output_cannot_be_in_two_groups_at_once() {
    let state = CoreState::new();
    let downstairs = state.sync.create_group("Downstairs");
    let party = state.sync.create_group("Party");
    let output = ActiveOutput {
        transport: "sonos".into(),
        device_id: "uuid:kitchen".into(),
        device_name: "Kitchen".into(),
    };
    let member = state.sync.join(&downstairs, &output).unwrap();

    let refused = state.sync.join(&party, &output);
    assert!(
        matches!(refused, Err(on_air_core::sync::JoinError::AlreadyInGroup)),
        "joining a second group must be refused, got {refused:?}"
    );
    let current = state
        .sync
        .group_of(&on_air_core::sync::OutputKey::new("sonos", "uuid:kitchen"))
        .expect("still a member of its first group");
    assert_eq!(current.id(), downstairs.id(), "the first group keeps it");
    assert_eq!(
        current.member_of(&on_air_core::sync::OutputKey::new("sonos", "uuid:kitchen")),
        Some(member)
    );

    // Re-joining the same group is idempotent, not a second membership.
    assert_eq!(state.sync.join(&downstairs, &output).unwrap(), member);
    assert_eq!(downstairs.members().len(), 1);
}

/// The hard refusal at the group boundary: while a sync-group member
/// session plays a receiver, no other session may claim it.
#[tokio::test]
async fn a_group_members_session_refuses_a_second_claimant() {
    let state = CoreState::new_mock().await;
    let created = on_air_core::sync::lifecycle::create(&state, "Downstairs")
        .await
        .unwrap();
    on_air_core::sync::lifecycle::add_member(&state, created, "sonos", "uuid:mock-sonos")
        .await
        .unwrap();
    on_air_core::sync::lifecycle::start(&state, created)
        .await
        .unwrap();

    let refused = on_air_core::session::activate(&state, "sonos", "uuid:mock-sonos").await;
    let error = refused.expect_err("a second claimant must be refused");
    assert!(
        matches!(
            error,
            on_air_core::session::ActivateError::Sender(SenderError::NotReady(_))
        ),
        "{error}"
    );
    assert!(
        error.to_string().contains("sync group"),
        "the refusal names the sync group: {error}"
    );
    state.shutdown().await;
}
