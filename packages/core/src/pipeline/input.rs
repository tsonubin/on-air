//! The live input: which capture feeds the pipeline PCM bus.
//!
//! [`InputSession`] owns the capture handle, the active input name, the
//! rates that input supports and the CD deck's producer attachment. Every
//! transition runs under one `tokio::sync::Mutex<InputState>`; readers use
//! the published [`InputSnapshot`] and never wait for a slow device start.
//!
//! If the processing thread panics, the session clears itself and
//! broadcasts `WsEvent::InputStateChanged` with the error.

use super::{
    capture, new_ring_buffer, spawn_processing_task_reporting, CaptureHandle, ProcessingOutputs,
    ProcessingRates, ProcessingStage,
};
use crate::cd::{CdDeck, ProducerGeneration, AUDIO_CD_INPUT, CD_SAMPLE_RATE_HZ};
use crate::dsp::rates;
use crate::events::WsEvent;
use bytes::Bytes;
use parking_lot::Mutex as SyncMutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::{broadcast, watch, Mutex};

const RING_BUFFER_DURATION_SECONDS: usize = 2;

/// Test-only fault injected into the processing thread.
#[cfg(test)]
type FaultStage = fn(&mut [f32]);

/// Virtual input name a client may POST to `/api/inputs/active` to mean
/// "whatever this OS exposes as the system-audio loopback". The core resolves
/// it to the preferred cpal loopback device, falling back to a Pulse monitor
/// source on Linux. Kept in sync with the desktop and mobile clients.
pub const LOOPBACK_INPUT: &str = "__loopback__";

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("{0}")]
    NotFound(&'static str),
    #[error("no audio compact disc")]
    NoDisc,
    #[error("{0}")]
    Capture(String),
}

/// Point-in-time view of the input session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSnapshot {
    /// The selected input. Still set after a failed restart so a later
    /// restart can retry it.
    pub name: Option<String>,
    /// Input rates the live device accepts.
    pub supported_hz: Vec<u32>,
    /// Why the capture is not running, when it died or failed to restart.
    pub error: Option<String>,
}

impl Default for InputSnapshot {
    fn default() -> Self {
        InputSnapshot {
            name: None,
            supported_hz: rates::INPUT_RATES_HZ.to_vec(),
            error: None,
        }
    }
}

/// A running capture plus what the session must undo when it stops.
struct Capture {
    handle: CaptureHandle,
    /// Set for the Audio CD input: the deck attachment this capture owns.
    cd_producer: Option<ProducerGeneration>,
    /// Identifies this capture in a processing-thread death report.
    id: u64,
}

enum InputState {
    Idle,
    /// Capturing. `capture` is `None` in mock mode, where nothing runs.
    Live {
        name: String,
        capture: Option<Box<Capture>>,
        supported_hz: Vec<u32>,
    },
    /// Selected but not running: a restart failed. `restart` retries.
    Broken {
        name: String,
        supported_hz: Vec<u32>,
        error: String,
    },
}

impl InputState {
    fn snapshot(&self) -> InputSnapshot {
        match self {
            InputState::Idle => InputSnapshot::default(),
            InputState::Live {
                name, supported_hz, ..
            } => InputSnapshot {
                name: Some(name.clone()),
                supported_hz: supported_hz.clone(),
                error: None,
            },
            InputState::Broken {
                name,
                supported_hz,
                error,
            } => InputSnapshot {
                name: Some(name.clone()),
                supported_hz: supported_hz.clone(),
                error: Some(error.clone()),
            },
        }
    }
}

/// What the input session feeds and reads.
pub struct InputWiring {
    pub audio_tx: broadcast::Sender<Bytes>,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub eq_gains_db: Arc<SyncMutex<[f32; 5]>>,
    /// The pipeline (input) rate the capture is converted to.
    pub input_rate_hz: Arc<SyncMutex<u32>>,
    pub cd: CdDeck,
}

struct Inner {
    state: Mutex<InputState>,
    published: watch::Sender<InputSnapshot>,
    wiring: InputWiring,
    next_capture_id: AtomicU64,
    mock: AtomicBool,
    mock_inputs: SyncMutex<Vec<String>>,
    #[cfg(test)]
    fault: SyncMutex<Option<FaultStage>>,
}

/// The live-input session. Cheap to clone; clones share one session.
#[derive(Clone)]
pub struct InputSession {
    inner: Arc<Inner>,
}

impl InputSession {
    pub fn new(wiring: InputWiring) -> Self {
        let (published, _) = watch::channel(InputSnapshot::default());
        InputSession {
            inner: Arc::new(Inner {
                state: Mutex::new(InputState::Idle),
                published,
                wiring,
                next_capture_id: AtomicU64::new(1),
                mock: AtomicBool::new(false),
                mock_inputs: SyncMutex::new(Vec::new()),
                #[cfg(test)]
                fault: SyncMutex::new(None),
            }),
        }
    }

    /// Mock mode: only `mock_inputs` (and an inserted CD) exist and nothing
    /// is captured.
    pub fn set_mock(&self, mock: bool) {
        self.inner.mock.store(mock, Ordering::Release);
    }

    pub fn set_mock_inputs(&self, inputs: Vec<String>) {
        *self.inner.mock_inputs.lock() = inputs;
    }

    pub fn mock_inputs(&self) -> Vec<String> {
        self.inner.mock_inputs.lock().clone()
    }

    pub fn snapshot(&self) -> InputSnapshot {
        self.inner.published.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<InputSnapshot> {
        self.inner.published.subscribe()
    }

    pub fn active_name(&self) -> Option<String> {
        self.snapshot().name
    }

    pub fn supported_hz(&self) -> Vec<u32> {
        self.snapshot().supported_hz
    }

    /// Label of the running capture handle, if one is running.
    pub async fn capture_label(&self) -> Option<String> {
        match &*self.inner.state.lock().await {
            InputState::Live {
                capture: Some(capture),
                ..
            } => Some(capture.handle.device_name.clone()),
            _ => None,
        }
    }

    /// Make `requested` the live input and return the resolved device name
    /// (the loopback alias resolves to a real device). The replacement is
    /// started before the current capture stops, so a failed start leaves
    /// the current input running.
    pub async fn activate(&self, requested: &str) -> Result<String, InputError> {
        let session = self.clone();
        let requested = requested.to_string();
        detached(async move { session.switch_to(requested).await }).await
    }

    /// Rebuild the live capture, e.g. after the pipeline rate changed.
    pub async fn restart(&self) -> Result<(), InputError> {
        let session = self.clone();
        detached(async move { session.rebuild().await }).await
    }

    /// Stop capturing and clear the input. Returns the input that stopped.
    pub async fn stop(&self) -> Option<String> {
        let session = self.clone();
        detached(async move { Ok(session.clear(None).await) })
            .await
            .unwrap_or(None)
    }

    /// Stop only if `name` is the live input. Returns whether it stopped.
    pub async fn stop_if_active(&self, name: &str) -> bool {
        let session = self.clone();
        let name = name.to_string();
        detached(async move { Ok(session.clear(Some(name)).await.is_some()) })
            .await
            .unwrap_or(false)
    }

    fn publish(&self, state: &InputState) {
        self.inner.published.send_replace(state.snapshot());
    }

    fn emit(&self, name: Option<String>, active: bool, error: Option<String>) {
        let _ = self.inner.wiring.ws_tx.send(WsEvent::InputStateChanged {
            name,
            active,
            error,
        });
    }

    async fn switch_to(&self, requested: String) -> Result<String, InputError> {
        if self.inner.mock.load(Ordering::Acquire) {
            let cd_ok = requested == AUDIO_CD_INPUT && self.inner.wiring.cd.status().present;
            let known = cd_ok || self.inner.mock_inputs.lock().contains(&requested);
            if !known {
                return Err(InputError::NotFound("input device not found"));
            }
            let mut state = self.inner.state.lock().await;
            *state = InputState::Live {
                name: requested.clone(),
                capture: None,
                supported_hz: self.snapshot().supported_hz,
            };
            self.publish(&state);
            self.emit(Some(requested.clone()), true, None);
            return Ok(requested);
        }

        let name = if requested == LOOPBACK_INPUT {
            resolve_loopback().await?
        } else {
            requested
        };
        let mut state = self.inner.state.lock().await;
        let (capture, supported_hz) = self.start_capture(&name).await?;
        let previous = std::mem::replace(
            &mut *state,
            InputState::Live {
                name: name.clone(),
                capture: Some(Box::new(capture)),
                supported_hz,
            },
        );
        self.teardown(previous).await;
        self.publish(&state);
        self.emit(Some(name.clone()), true, None);
        Ok(name)
    }

    async fn rebuild(&self) -> Result<(), InputError> {
        if self.inner.mock.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut state = self.inner.state.lock().await;
        let (name, supported_hz) = match &*state {
            InputState::Idle => return Ok(()),
            InputState::Live {
                name, supported_hz, ..
            }
            | InputState::Broken {
                name, supported_hz, ..
            } => (name.clone(), supported_hz.clone()),
        };
        // The device is exclusive on some hosts: stop before reopening.
        let previous = std::mem::replace(&mut *state, InputState::Idle);
        self.teardown(previous).await;
        match self.start_capture(&name).await {
            Ok((capture, supported_hz)) => {
                *state = InputState::Live {
                    name,
                    capture: Some(Box::new(capture)),
                    supported_hz,
                };
                self.publish(&state);
                Ok(())
            }
            Err(error) => {
                *state = InputState::Broken {
                    name,
                    supported_hz,
                    error: error.to_string(),
                };
                self.publish(&state);
                Err(error)
            }
        }
    }

    async fn clear(&self, only: Option<String>) -> Option<String> {
        let mut state = self.inner.state.lock().await;
        let name = match &*state {
            InputState::Idle => return None,
            InputState::Live { name, .. } | InputState::Broken { name, .. } => name.clone(),
        };
        if only.as_ref().is_some_and(|only| *only != name) {
            return None;
        }
        let previous = std::mem::replace(&mut *state, InputState::Idle);
        self.teardown(previous).await;
        self.publish(&state);
        self.emit(Some(name.clone()), false, None);
        Some(name)
    }

    /// Called (on the runtime) after a processing thread panicked.
    async fn processing_died(&self, capture_id: u64, error: String) {
        let mut state = self.inner.state.lock().await;
        let name = match &*state {
            InputState::Live {
                name,
                capture: Some(capture),
                ..
            } if capture.id == capture_id => name.clone(),
            // A capture that was already replaced or stopped.
            _ => return,
        };
        let previous = std::mem::replace(&mut *state, InputState::Idle);
        self.teardown(previous).await;
        eprintln!("input {name:?} stopped: {error}");
        self.inner.published.send_replace(InputSnapshot {
            error: Some(error.clone()),
            ..InputSnapshot::default()
        });
        self.emit(Some(name), false, Some(error));
    }

    async fn teardown(&self, previous: InputState) {
        let InputState::Live {
            capture: Some(capture),
            ..
        } = previous
        else {
            return;
        };
        let Capture {
            handle,
            cd_producer,
            ..
        } = *capture;
        let _ = tokio::task::spawn_blocking(move || handle.stop()).await;
        if let Some(generation) = cd_producer {
            // A newer attachment (the replacement CD capture) is left alone.
            self.inner.wiring.cd.detach_producer(generation);
        }
    }

    fn death_hook(&self, capture_id: u64) -> super::ProcessingDeathHook {
        let session: Weak<Inner> = Arc::downgrade(&self.inner);
        let runtime = tokio::runtime::Handle::current();
        Box::new(move |error| {
            let Some(inner) = session.upgrade() else {
                return;
            };
            let session = InputSession { inner };
            runtime.spawn(async move { session.processing_died(capture_id, error).await });
        })
    }

    fn fault_stage(&self) -> Option<ProcessingStage> {
        #[cfg(test)]
        if let Some(fault) = *self.inner.fault.lock() {
            return Some(Box::new(fault));
        }
        None
    }

    fn spawn_processing(
        &self,
        consumer: ringbuf::HeapCons<f32>,
        input_hz: u32,
        output_hz: u32,
        capture_id: u64,
    ) -> super::ProcessingTaskHandle {
        let wiring = &self.inner.wiring;
        spawn_processing_task_reporting(
            consumer,
            ProcessingRates {
                input_hz,
                output_hz,
            },
            ProcessingOutputs {
                eq_gains_db: wiring.eq_gains_db.clone(),
                audio_tx: wiring.audio_tx.clone(),
                ws_tx: wiring.ws_tx.clone(),
            },
            self.fault_stage(),
            Some(self.death_hook(capture_id)),
        )
    }

    async fn start_capture(&self, name: &str) -> Result<(Capture, Vec<u32>), InputError> {
        let target_rate = *self.inner.wiring.input_rate_hz.lock();
        let ring_capacity = (target_rate as usize).saturating_mul(RING_BUFFER_DURATION_SECONDS);
        let (producer, consumer) = new_ring_buffer(ring_capacity);
        let id = self.inner.next_capture_id.fetch_add(1, Ordering::AcqRel);
        let label = name.to_string();

        if name == AUDIO_CD_INPUT {
            let cd = &self.inner.wiring.cd;
            if !cd.status().present {
                return Err(InputError::NoDisc);
            }
            let generation = cd.attach_producer(producer);
            let processing = self.spawn_processing(consumer, CD_SAMPLE_RATE_HZ, target_rate, id);
            return Ok((
                Capture {
                    handle: CaptureHandle::processing_only(processing, label),
                    cd_producer: Some(generation),
                    id,
                },
                rates::INPUT_RATES_HZ.to_vec(),
            ));
        }

        if capture::is_pulse_monitor_name(name) {
            let src = name.to_string();
            let started = tokio::task::spawn_blocking(move || {
                capture::start_pulse_monitor(&src, producer, Some(target_rate))
            })
            .await
            .map_err(|e| InputError::Capture(e.to_string()))?;
            let (pulse, input_rate) = started.map_err(InputError::Capture)?;
            let processing = self.spawn_processing(consumer, input_rate, target_rate, id);
            return Ok((
                Capture {
                    handle: CaptureHandle::pulse(pulse, processing, label),
                    cd_producer: None,
                    id,
                },
                rates::INPUT_RATES_HZ.to_vec(),
            ));
        }

        let device_name = name.to_string();
        let started = tokio::task::spawn_blocking(move || {
            let host = cpal::default_host();
            let Some(device) =
                capture::find_input_device(&host, &device_name).map_err(|e| e.to_string())?
            else {
                return Ok(None);
            };
            let supported_ranges = capture::supported_input_rate_ranges(&device);
            capture::start_capture_at(&device, producer, Some(target_rate))
                .map(|(stream, input_rate)| Some((stream, input_rate, supported_ranges)))
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| InputError::Capture(e.to_string()))?;
        let Some((stream, input_rate, supported_ranges)) = started.map_err(InputError::Capture)?
        else {
            return Err(InputError::NotFound("input device not found"));
        };
        let supported_hz = rates::intersect_catalog(&supported_ranges, rates::INPUT_RATES_HZ);
        let processing = self.spawn_processing(consumer, input_rate, target_rate, id);
        Ok((
            Capture {
                handle: CaptureHandle::cpal(stream, processing, label),
                cd_producer: None,
                id,
            },
            supported_hz,
        ))
    }

    #[cfg(test)]
    fn inject_processing_fault(&self, fault: FaultStage) {
        *self.inner.fault.lock() = Some(fault);
    }
}

async fn resolve_loopback() -> Result<String, InputError> {
    let resolved = tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        capture::find_preferred_loopback(&host).map(|device| {
            device
                .map(|device| device.to_string())
                .or_else(capture::preferred_pulse_monitor)
        })
    })
    .await;
    match resolved {
        Ok(Ok(Some(name))) => Ok(name),
        Ok(Ok(None)) => Err(InputError::NotFound("no loopback capture device")),
        Ok(Err(error)) => Err(InputError::Capture(error.to_string())),
        Err(error) => Err(InputError::Capture(error.to_string())),
    }
}

/// Run a transition on its own task so a dropped request cannot strand a
/// started capture (whose processing thread would then never stop).
async fn detached<T, F>(transition: F) -> Result<T, InputError>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, InputError>> + Send + 'static,
{
    match tokio::spawn(transition).await {
        Ok(result) => result,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => Err(InputError::Capture(format!(
            "input transition did not finish: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cd::GeneratedCd;
    use std::time::Duration;

    fn session() -> (InputSession, broadcast::Receiver<WsEvent>, CdDeck) {
        let (audio_tx, _) = broadcast::channel(64);
        let (ws_tx, ws_rx) = broadcast::channel(256);
        let cd = CdDeck::new();
        let session = InputSession::new(InputWiring {
            audio_tx,
            ws_tx,
            eq_gains_db: Arc::new(SyncMutex::new([0.0; 5])),
            input_rate_hz: Arc::new(SyncMutex::new(44_100)),
            cd: cd.clone(),
        });
        (session, ws_rx, cd)
    }

    fn insert_disc(cd: &CdDeck) {
        let medium = GeneratedCd::from_titles(Some("Album".into()), &["A".to_string()], 60_000);
        cd.set_medium(Some(Arc::new(medium)));
    }

    async fn next_input_event(
        events: &mut broadcast::Receiver<WsEvent>,
    ) -> (Option<String>, bool, Option<String>) {
        loop {
            match events.recv().await.unwrap() {
                WsEvent::InputStateChanged {
                    name,
                    active,
                    error,
                } => return (name, active, error),
                _ => continue,
            }
        }
    }

    #[tokio::test]
    async fn a_panicking_processing_thread_clears_the_input_and_reports_it() {
        let (session, mut events, cd) = session();
        insert_disc(&cd);
        session.inject_processing_fault(|_| panic!("injected stage fault"));

        session.activate(AUDIO_CD_INPUT).await.unwrap();
        assert_eq!(
            next_input_event(&mut events).await,
            (Some(AUDIO_CD_INPUT.to_string()), true, None)
        );
        cd.play();

        let (name, active, error) =
            tokio::time::timeout(Duration::from_secs(5), next_input_event(&mut events))
                .await
                .expect("processing death was never reported");
        assert_eq!(name.as_deref(), Some(AUDIO_CD_INPUT));
        assert!(!active);
        let error = error.expect("death carries the error");
        assert!(error.contains("injected stage fault"), "{error}");

        let snapshot = session.snapshot();
        assert_eq!(snapshot.name, None);
        assert_eq!(snapshot.error.as_deref(), Some(error.as_str()));
        assert!(session.capture_label().await.is_none());
        assert!(
            !cd.has_producer(),
            "the dead capture's producer is detached"
        );
    }

    #[tokio::test]
    async fn a_stale_death_report_does_not_clear_a_newer_capture() {
        let (session, _events, cd) = session();
        insert_disc(&cd);
        session.activate(AUDIO_CD_INPUT).await.unwrap();
        session.processing_died(u64::MAX, "old thread".into()).await;
        assert_eq!(session.active_name().as_deref(), Some(AUDIO_CD_INPUT));
        assert!(cd.has_producer());
        session.stop().await;
    }

    #[tokio::test]
    async fn reactivating_the_cd_input_keeps_the_fresh_producer_attached() {
        let (session, _events, cd) = session();
        insert_disc(&cd);
        session.activate(AUDIO_CD_INPUT).await.unwrap();
        session.activate(AUDIO_CD_INPUT).await.unwrap();
        assert!(cd.has_producer());
        assert_eq!(
            session.capture_label().await.as_deref(),
            Some(AUDIO_CD_INPUT)
        );
        assert_eq!(session.stop().await.as_deref(), Some(AUDIO_CD_INPUT));
        assert!(!cd.has_producer());
        assert_eq!(session.snapshot(), InputSnapshot::default());
    }

    #[tokio::test]
    async fn stop_if_active_leaves_another_input_alone() {
        let (session, _events, _cd) = session();
        session.set_mock(true);
        session.set_mock_inputs(vec!["Mic".into()]);
        session.activate("Mic").await.unwrap();
        assert!(!session.stop_if_active(AUDIO_CD_INPUT).await);
        assert_eq!(session.active_name().as_deref(), Some("Mic"));
        assert!(session.stop_if_active("Mic").await);
        assert_eq!(session.active_name(), None);
    }

    #[tokio::test]
    async fn a_cd_without_a_disc_is_no_disc() {
        let (session, _events, _cd) = session();
        assert!(matches!(
            session.activate(AUDIO_CD_INPUT).await,
            Err(InputError::NoDisc)
        ));
        assert_eq!(session.snapshot(), InputSnapshot::default());
    }
}
