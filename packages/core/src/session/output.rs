//! The exclusive output: at most one sender is live, and switching tears the
//! previous one down first.
//!
//! [`OutputSession`] owns the active sender, its identity, its negotiated
//! format, the radio stream nonce and the local-sink policy. Every
//! transition runs under one `tokio::sync::Mutex<OutputState>`, so two
//! concurrent activations are serialized and can never both end up live.
//!
//! Readers never take that lock: each transition publishes an
//! [`OutputSnapshot`] that the HTTP handlers, the radio stream and the
//! sample-rate API read without waiting for a slow `start()`. Only the
//! holder of the transition lock writes the snapshot.

use crate::events::WsEvent;
use crate::sender::{AudioSender, OutputFormat, SenderError};
use parking_lot::Mutex as SyncMutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, Mutex, MutexGuard};

/// Which speaker the exclusive output belongs to. Persisted in the saved
/// settings, so its serialized shape is part of the settings file format.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ActiveOutput {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
}

/// Lifecycle of the exclusive output as clients see it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputPhase {
    #[default]
    Idle,
    /// `start()` is in flight. The identity and stream nonce are already
    /// visible because Sonos pulls the radio URI during Play.
    Starting,
    Live,
    /// Start (or a rollback) failed and the sender's cleanup also failed.
    /// The sender is retained so a later stop can retry; nothing else may
    /// start until it does.
    Failed,
}

/// Point-in-time view of the output session, consistent across fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputSnapshot {
    pub phase: OutputPhase,
    pub identity: Option<ActiveOutput>,
    pub format: Option<OutputFormat>,
    /// Path segment of the live radio stream (`/stream/<nonce>/audio.wav`).
    pub nonce: Option<String>,
    /// Why the session is `Failed`.
    pub error: Option<String>,
}

impl OutputSnapshot {
    /// True while a Sonos or AirPlay output owns `nonce`. Bluetooth plays
    /// through the OS and never exposes the radio.
    pub fn serves_radio(&self, nonce: &str) -> bool {
        let pulls_radio = self
            .identity
            .as_ref()
            .is_some_and(|o| o.transport == "sonos" || o.transport == "airplay");
        pulls_radio && self.nonce.as_deref() == Some(nonce)
    }
}

/// The session's state machine. `Starting` carries no sender: the sender is
/// owned by the transition that is starting it.
enum OutputState {
    Idle,
    Starting {
        identity: ActiveOutput,
        nonce: Option<String>,
    },
    Live {
        sender: Box<dyn AudioSender>,
        identity: ActiveOutput,
        format: Option<OutputFormat>,
        nonce: Option<String>,
    },
    Failed {
        sender: Box<dyn AudioSender>,
        identity: ActiveOutput,
        nonce: Option<String>,
        error: String,
    },
}

impl OutputState {
    fn snapshot(&self) -> OutputSnapshot {
        match self {
            OutputState::Idle => OutputSnapshot::default(),
            OutputState::Starting { identity, nonce } => OutputSnapshot {
                phase: OutputPhase::Starting,
                identity: Some(identity.clone()),
                format: None,
                nonce: nonce.clone(),
                error: None,
            },
            OutputState::Live {
                identity,
                format,
                nonce,
                ..
            } => OutputSnapshot {
                phase: OutputPhase::Live,
                identity: Some(identity.clone()),
                format: format.clone(),
                nonce: nonce.clone(),
                error: None,
            },
            OutputState::Failed {
                identity,
                nonce,
                error,
                ..
            } => OutputSnapshot {
                phase: OutputPhase::Failed,
                identity: Some(identity.clone()),
                format: None,
                nonce: nonce.clone(),
                error: Some(error.clone()),
            },
        }
    }

    fn sender_mut(&mut self) -> Option<&mut Box<dyn AudioSender>> {
        match self {
            OutputState::Live { sender, .. } | OutputState::Failed { sender, .. } => Some(sender),
            OutputState::Idle | OutputState::Starting { .. } => None,
        }
    }
}

/// A sender that was live (or failed) and has just been stopped, kept so a
/// failed switch can restart it.
struct Stopped {
    sender: Box<dyn AudioSender>,
    identity: ActiveOutput,
    nonce: Option<String>,
}

struct Inner {
    state: Mutex<OutputState>,
    published: watch::Sender<OutputSnapshot>,
    ws_tx: broadcast::Sender<WsEvent>,
    /// The requested output rate; a live sender's negotiated rate wins.
    output_rate_hz: Arc<SyncMutex<u32>>,
    /// Mute laptop speakers while a remote output is live. Off in mock mode.
    manage_local_sink: AtomicBool,
    #[cfg(test)]
    after_starting: SyncMutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

/// The exclusive-output session. Cheap to clone; clones share one session.
#[derive(Clone)]
pub struct OutputSession {
    inner: Arc<Inner>,
}

impl OutputSession {
    pub fn new(ws_tx: broadcast::Sender<WsEvent>, output_rate_hz: Arc<SyncMutex<u32>>) -> Self {
        let (published, _) = watch::channel(OutputSnapshot::default());
        OutputSession {
            inner: Arc::new(Inner {
                state: Mutex::new(OutputState::Idle),
                published,
                ws_tx,
                output_rate_hz,
                manage_local_sink: AtomicBool::new(true),
                #[cfg(test)]
                after_starting: SyncMutex::new(None),
            }),
        }
    }

    /// Whether switching outputs mutes and restores the laptop's own
    /// speakers. Disabled for the hardware-free mock core.
    pub fn set_manage_local_sink(&self, enabled: bool) {
        self.inner
            .manage_local_sink
            .store(enabled, Ordering::Release);
    }

    /// The current state without waiting for an in-flight transition.
    pub fn snapshot(&self) -> OutputSnapshot {
        self.inner.published.borrow().clone()
    }

    /// Follow snapshots as they are published (the radio stream uses this to
    /// end a reader when its nonce is retired).
    pub fn subscribe(&self) -> watch::Receiver<OutputSnapshot> {
        self.inner.published.subscribe()
    }

    /// The identity of the output that currently owns exclusivity, if any.
    pub fn active(&self) -> Option<ActiveOutput> {
        self.snapshot().identity
    }

    /// Name of the retained sender (live or failed). Waits for an in-flight
    /// transition.
    pub async fn sender_name(&self) -> Option<String> {
        let mut state = self.inner.state.lock().await;
        state.sender_mut().map(|sender| sender.name().to_string())
    }

    /// Exclusive-output switch: stop the current sender, then start
    /// `sender`. `nonce` is the radio path segment its receiver will pull;
    /// it goes live with the identity before `start()` because Sonos fetches
    /// the URI during Play. The previous nonce stays valid until the old
    /// sender's Stop has succeeded.
    ///
    /// The transition runs on its own task, so a dropped HTTP request cannot
    /// abandon it half-way.
    pub async fn activate(
        &self,
        sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
        nonce: Option<String>,
    ) -> Result<(), SenderError> {
        let session = self.clone();
        run_detached(async move { session.switch_to(sender, identity, nonce).await }).await
    }

    /// Stop the live sender, if any, and restore the local speakers.
    pub async fn deactivate(&self) -> Result<(), SenderError> {
        let session = self.clone();
        run_detached(async move { session.stop_live().await }).await
    }

    /// Set the volume on the retained sender.
    pub async fn set_volume(&self, volume: u8) -> Result<(), SenderError> {
        let mut state = self.inner.state.lock().await;
        match state.sender_mut() {
            Some(sender) => sender.set_volume(volume).await,
            None => Err(SenderError::NoActiveOutput),
        }
    }

    /// Drop whatever sender is retained without stopping it. Shutdown uses
    /// this after a failed stop so the process can still exit.
    pub async fn abandon(&self) {
        let mut state = self.inner.state.lock().await;
        let abandoned = std::mem::replace(&mut *state, OutputState::Idle);
        self.publish(&state);
        drop(state);
        drop(abandoned);
    }

    fn publish(&self, state: &OutputState) {
        self.inner.published.send_replace(state.snapshot());
    }

    fn set(&self, state: &mut OutputState, next: OutputState) {
        *state = next;
        self.publish(state);
    }

    fn emit(&self, identity: &ActiveOutput, active: bool) {
        let _ = self.inner.ws_tx.send(WsEvent::OutputStateChanged {
            transport: identity.transport.clone(),
            device_name: identity.device_name.clone(),
            active,
        });
    }

    /// Stop the retained sender in place. On failure the state is left
    /// exactly as it was (the old nonce stays valid). On success the session
    /// is `Idle` and the stopped sender is returned for a possible rollback.
    async fn stop_current(
        &self,
        state: &mut MutexGuard<'_, OutputState>,
    ) -> Result<Option<Stopped>, SenderError> {
        if let Some(sender) = state.sender_mut() {
            sender.stop().await?;
        }
        let stopped = match std::mem::replace(&mut **state, OutputState::Idle) {
            OutputState::Live {
                sender,
                identity,
                nonce,
                ..
            }
            | OutputState::Failed {
                sender,
                identity,
                nonce,
                ..
            } => Some(Stopped {
                sender,
                identity,
                nonce,
            }),
            // `Starting` only survives a transition that was torn down
            // mid-way; there is no sender to stop.
            OutputState::Starting { .. } | OutputState::Idle => None,
        };
        Ok(stopped)
    }

    async fn stop_live(&self) -> Result<(), SenderError> {
        let mut state = self.inner.state.lock().await;
        let stopped = self.stop_current(&mut state).await?;
        self.publish(&state);
        if let Some(stopped) = stopped {
            self.restore_local_sink().await;
            self.emit(&stopped.identity, false);
        }
        Ok(())
    }

    async fn switch_to(
        &self,
        mut sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
        nonce: Option<String>,
    ) -> Result<(), SenderError> {
        let mut state = self.inner.state.lock().await;
        let previous = self
            .stop_current(&mut state)
            .await
            .map_err(|error| SenderError::StopFailed(Box::new(error)))?;
        if let Some(previous) = previous.as_ref() {
            self.emit(&previous.identity, false);
        }
        self.publish(&state);

        let identity = identity.unwrap_or_else(|| ActiveOutput {
            transport: sender.transport().to_string(),
            device_id: String::new(),
            device_name: sender.name().to_string(),
        });
        // GET /stream/<nonce>/audio.wav is 404 until both are visible.
        self.set(
            &mut state,
            OutputState::Starting {
                identity: identity.clone(),
                nonce: nonce.clone(),
            },
        );
        #[cfg(test)]
        if let Some(hook) = self.inner.after_starting.lock().as_ref() {
            hook();
        }

        let error = match sender.start().await {
            Ok(()) => {
                self.go_live(&mut state, sender, identity, nonce).await;
                return Ok(());
            }
            Err(error) => error,
        };

        if let Err(cleanup) = sender.stop().await {
            // A receiver may still be playing after a partial start. Keep
            // ownership for another stop attempt; never resume a second one.
            self.set(
                &mut state,
                OutputState::Failed {
                    sender,
                    identity,
                    nonce,
                    error: error.to_string(),
                },
            );
            return Err(SenderError::RollbackFailed {
                error: Box::new(error),
                rollback: format!("cleanup after failed start also failed: {cleanup}"),
            });
        }
        self.set(&mut state, OutputState::Idle);

        let Some(Stopped {
            sender: mut previous,
            identity: previous_identity,
            nonce: previous_nonce,
        }) = previous
        else {
            self.restore_local_sink().await;
            return Err(error);
        };

        self.set(
            &mut state,
            OutputState::Starting {
                identity: previous_identity.clone(),
                nonce: previous_nonce.clone(),
            },
        );
        if let Err(restart) = previous.start().await {
            if let Err(cleanup) = previous.stop().await {
                self.set(
                    &mut state,
                    OutputState::Failed {
                        sender: previous,
                        identity: previous_identity,
                        nonce: previous_nonce,
                        error: restart.to_string(),
                    },
                );
                return Err(SenderError::RollbackFailed {
                    error: Box::new(error),
                    rollback: format!(
                        "restoring previous output failed: {restart}; cleanup failed: {cleanup}"
                    ),
                });
            }
            self.set(&mut state, OutputState::Idle);
            self.restore_local_sink().await;
            return Err(SenderError::RollbackFailed {
                error: Box::new(error),
                rollback: format!("restoring previous output failed: {restart}"),
            });
        }
        self.go_live(&mut state, previous, previous_identity, previous_nonce)
            .await;
        Err(error)
    }

    async fn go_live(
        &self,
        state: &mut OutputState,
        sender: Box<dyn AudioSender>,
        identity: ActiveOutput,
        nonce: Option<String>,
    ) {
        let format = sender.output_format();
        if let Some(format) = &format {
            *self.inner.output_rate_hz.lock() = format.sample_rate_hz;
        }
        self.set(
            state,
            OutputState::Live {
                sender,
                identity: identity.clone(),
                format,
                nonce,
            },
        );
        self.apply_local_sink(&identity).await;
        self.emit(&identity, true);
    }

    async fn apply_local_sink(&self, identity: &ActiveOutput) {
        if !self.inner.manage_local_sink.load(Ordering::Acquire) {
            return;
        }
        let transport = identity.transport.clone();
        let device_id = identity.device_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            crate::pipeline::local_sink::apply_for_transport(&transport, &device_id);
        })
        .await;
    }

    /// Unmute the laptop speakers. Also used by shutdown.
    pub async fn restore_local_sink(&self) {
        if !self.inner.manage_local_sink.load(Ordering::Acquire) {
            return;
        }
        let _ = tokio::task::spawn_blocking(|| {
            crate::pipeline::local_sink::restore_local_speakers();
        })
        .await;
    }

    #[cfg(test)]
    fn on_starting(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.inner.after_starting.lock() = Some(Box::new(hook));
    }
}

/// Run a transition on its own task so cancelling the caller (a dropped
/// HTTP request) cannot leave the session half-switched. A panic inside the
/// transition is resumed in the caller.
async fn run_detached<F>(transition: F) -> Result<(), SenderError>
where
    F: std::future::Future<Output = Result<(), SenderError>> + Send + 'static,
{
    match tokio::spawn(transition).await {
        Ok(result) => result,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => Err(SenderError::internal(format!(
            "output transition did not finish: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::NullSender;
    use std::time::Duration;

    type Log = Arc<tokio::sync::Mutex<Vec<String>>>;

    fn session() -> (OutputSession, broadcast::Receiver<WsEvent>) {
        let (ws_tx, ws_rx) = broadcast::channel(64);
        let session = OutputSession::new(ws_tx, Arc::new(SyncMutex::new(44_100)));
        session.set_manage_local_sink(false);
        (session, ws_rx)
    }

    fn identity(id: &str) -> ActiveOutput {
        ActiveOutput {
            transport: "sonos".into(),
            device_id: id.into(),
            device_name: format!("Speaker {id}"),
        }
    }

    /// Scripted sender: optional start/stop failures and an optional start
    /// delay so tests can interleave transitions.
    struct Scripted {
        name: String,
        log: Log,
        fail_start: bool,
        fail_stop: bool,
        start_delay: Duration,
        format: Option<OutputFormat>,
    }

    impl Scripted {
        fn new(name: &str, log: &Log) -> Self {
            Scripted {
                name: name.into(),
                log: log.clone(),
                fail_start: false,
                fail_stop: false,
                start_delay: Duration::ZERO,
                format: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl AudioSender for Scripted {
        async fn start(&mut self) -> Result<(), SenderError> {
            tokio::time::sleep(self.start_delay).await;
            self.log.lock().await.push(format!("{}:start", self.name));
            if self.fail_start {
                Err(SenderError::transport(format!("{} refused", self.name)))
            } else {
                Ok(())
            }
        }
        async fn stop(&mut self) -> Result<(), SenderError> {
            self.log.lock().await.push(format!("{}:stop", self.name));
            if self.fail_stop {
                Err(SenderError::transport(format!("{} stuck", self.name)))
            } else {
                Ok(())
            }
        }
        async fn set_volume(&mut self, _: u8) -> Result<(), SenderError> {
            Ok(())
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn transport(&self) -> &'static str {
            "sonos"
        }
        fn output_format(&self) -> Option<OutputFormat> {
            self.format.clone()
        }
    }

    #[tokio::test]
    async fn a_successful_switch_goes_idle_starting_live() {
        let (session, mut events) = session();
        let log = Log::default();
        let mut seen = session.subscribe();
        assert_eq!(session.snapshot(), OutputSnapshot::default());

        let mut sender = Scripted::new("A", &log);
        sender.format = Some(OutputFormat {
            sample_rate_hz: 48_000,
            supported_hz: vec![48_000],
        });
        session
            .activate(Box::new(sender), Some(identity("a")), Some("n1".into()))
            .await
            .unwrap();

        let snapshot = session.snapshot();
        assert_eq!(snapshot.phase, OutputPhase::Live);
        assert_eq!(snapshot.identity, Some(identity("a")));
        assert_eq!(snapshot.nonce.as_deref(), Some("n1"));
        assert_eq!(snapshot.format.unwrap().sample_rate_hz, 48_000);
        assert_eq!(*session.inner.output_rate_hz.lock(), 48_000);
        assert!(seen.has_changed().unwrap());
        assert_eq!(seen.borrow_and_update().phase, OutputPhase::Live);
        assert!(matches!(
            events.try_recv().unwrap(),
            WsEvent::OutputStateChanged { active: true, .. }
        ));

        session.deactivate().await.unwrap();
        assert_eq!(session.snapshot(), OutputSnapshot::default());
        assert!(matches!(
            events.try_recv().unwrap(),
            WsEvent::OutputStateChanged { active: false, .. }
        ));
        assert_eq!(*log.lock().await, ["A:start", "A:stop"]);
    }

    #[tokio::test]
    async fn the_snapshot_shows_starting_while_start_is_in_flight() {
        let (session, _events) = session();
        let log = Log::default();
        let mut sender = Scripted::new("Slow", &log);
        sender.start_delay = Duration::from_millis(200);

        let activating = session.clone();
        let task = tokio::spawn(async move {
            activating
                .activate(Box::new(sender), Some(identity("slow")), Some("n".into()))
                .await
        });
        let mut rx = session.subscribe();
        tokio::time::timeout(
            Duration::from_secs(1),
            rx.wait_for(|s| s.phase == OutputPhase::Starting),
        )
        .await
        .expect("never published Starting")
        .unwrap();

        // Readers do not wait for the transition lock.
        let during = session.snapshot();
        assert_eq!(during.phase, OutputPhase::Starting);
        assert_eq!(during.identity, Some(identity("slow")));
        assert!(
            during.serves_radio("n"),
            "Sonos must reach the radio during Play"
        );
        assert!(during.format.is_none());

        task.await.unwrap().unwrap();
        assert_eq!(session.snapshot().phase, OutputPhase::Live);
    }

    #[tokio::test]
    async fn a_failed_start_with_failed_cleanup_is_retained_as_failed() {
        let (session, _events) = session();
        let log = Log::default();
        let mut sender = Scripted::new("Bad", &log);
        sender.fail_start = true;
        sender.fail_stop = true;

        let error = session
            .activate(Box::new(sender), Some(identity("bad")), Some("n".into()))
            .await
            .unwrap_err();
        assert!(
            matches!(error, SenderError::RollbackFailed { .. }),
            "{error}"
        );

        let snapshot = session.snapshot();
        assert_eq!(snapshot.phase, OutputPhase::Failed);
        assert_eq!(snapshot.identity, Some(identity("bad")));
        assert_eq!(snapshot.nonce.as_deref(), Some("n"));
        assert_eq!(snapshot.error.as_deref(), Some("Bad refused"));
        assert_eq!(session.sender_name().await.as_deref(), Some("Bad"));

        // Nothing else may start while the failed sender cannot be stopped.
        let blocked = session
            .activate(Box::new(Scripted::new("Next", &log)), None, None)
            .await
            .unwrap_err();
        assert!(matches!(blocked, SenderError::StopFailed(_)), "{blocked}");
        assert_eq!(session.snapshot().phase, OutputPhase::Failed);
        assert_eq!(*log.lock().await, ["Bad:start", "Bad:stop", "Bad:stop"]);
    }

    #[tokio::test]
    async fn a_failed_start_restores_the_previous_output() {
        let (session, _events) = session();
        let log = Log::default();
        session
            .activate(
                Box::new(Scripted::new("A", &log)),
                Some(identity("a")),
                Some("old".into()),
            )
            .await
            .unwrap();

        let mut bad = Scripted::new("B", &log);
        bad.fail_start = true;
        let error = session
            .activate(Box::new(bad), Some(identity("b")), Some("new".into()))
            .await
            .unwrap_err();
        assert!(matches!(error, SenderError::Transport(_)), "{error}");

        let snapshot = session.snapshot();
        assert_eq!(snapshot.phase, OutputPhase::Live);
        assert_eq!(snapshot.identity, Some(identity("a")));
        assert_eq!(snapshot.nonce.as_deref(), Some("old"));
        assert_eq!(
            *log.lock().await,
            ["A:start", "A:stop", "B:start", "B:stop", "A:start"]
        );
    }

    #[tokio::test]
    async fn a_failed_start_without_a_previous_output_goes_idle() {
        let (session, _events) = session();
        let log = Log::default();
        let mut bad = Scripted::new("B", &log);
        bad.fail_start = true;
        assert!(session
            .activate(Box::new(bad), Some(identity("b")), Some("n".into()))
            .await
            .is_err());
        assert_eq!(session.snapshot(), OutputSnapshot::default());
        assert!(session.sender_name().await.is_none());
    }

    #[tokio::test]
    async fn the_previous_nonce_stays_valid_until_its_stop_succeeds() {
        let (session, _events) = session();
        let log = Log::default();
        let mut stuck = Scripted::new("A", &log);
        stuck.fail_stop = true;
        session
            .activate(Box::new(stuck), Some(identity("a")), Some("old".into()))
            .await
            .unwrap();

        let error = session
            .activate(
                Box::new(Scripted::new("B", &log)),
                Some(identity("b")),
                Some("new".into()),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, SenderError::StopFailed(_)), "{error}");
        let snapshot = session.snapshot();
        assert_eq!(snapshot.phase, OutputPhase::Live);
        assert!(snapshot.serves_radio("old"));
        assert!(!snapshot.serves_radio("new"));
    }

    #[tokio::test]
    async fn concurrent_activations_leave_exactly_one_sender_live() {
        let (session, mut events) = session();
        let log = Log::default();

        // Hold the first transition inside `start()` until the second one is
        // queued on the lock.
        let mut first = Scripted::new("A", &log);
        first.start_delay = Duration::from_millis(100);
        let a = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .activate(Box::new(first), Some(identity("a")), Some("na".into()))
                    .await
            })
        };
        let mut rx = session.subscribe();
        rx.wait_for(|s| s.phase == OutputPhase::Starting)
            .await
            .unwrap();
        let b = {
            let session = session.clone();
            let second = Scripted::new("B", &log);
            tokio::spawn(async move {
                session
                    .activate(Box::new(second), Some(identity("b")), Some("nb".into()))
                    .await
            })
        };
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();

        // A started, was stopped cleanly by B, then B started. Never both.
        assert_eq!(*log.lock().await, ["A:start", "A:stop", "B:start"]);
        let snapshot = session.snapshot();
        assert_eq!(snapshot.phase, OutputPhase::Live);
        assert_eq!(snapshot.identity, Some(identity("b")));
        assert_eq!(session.sender_name().await.as_deref(), Some("B"));

        let mut transitions = Vec::new();
        while let Ok(WsEvent::OutputStateChanged {
            device_name,
            active,
            ..
        }) = events.try_recv()
        {
            transitions.push((device_name, active));
        }
        assert_eq!(
            transitions,
            [
                ("Speaker a".to_string(), true),
                ("Speaker a".to_string(), false),
                ("Speaker b".to_string(), true),
            ]
        );
    }

    #[tokio::test]
    async fn a_cancelled_caller_does_not_abandon_the_transition() {
        let (session, _events) = session();
        let log = Log::default();
        let mut slow = Scripted::new("A", &log);
        slow.start_delay = Duration::from_millis(100);

        let started = Arc::new(AtomicBool::new(false));
        let flag = started.clone();
        session.on_starting(move || flag.store(true, Ordering::Release));
        let caller = session.activate(Box::new(slow), Some(identity("a")), None);
        // Drop the caller while `start()` is in flight.
        let _ = tokio::time::timeout(Duration::from_millis(20), caller).await;
        assert!(started.load(Ordering::Acquire));

        let mut rx = session.subscribe();
        tokio::time::timeout(
            Duration::from_secs(1),
            rx.wait_for(|s| s.phase == OutputPhase::Live),
        )
        .await
        .expect("detached transition finished")
        .unwrap();
        assert_eq!(session.sender_name().await.as_deref(), Some("A"));
    }

    #[tokio::test]
    async fn set_volume_without_a_sender_is_no_active_output() {
        let (session, _events) = session();
        assert!(matches!(
            session.set_volume(10).await,
            Err(SenderError::NoActiveOutput)
        ));
        let log = Log::default();
        session
            .activate(Box::new(NullSender::new("A", log.clone())), None, None)
            .await
            .unwrap();
        session.set_volume(10).await.unwrap();
        assert_eq!(*log.lock().await, ["A:start", "A:volume:10"]);
    }
}
