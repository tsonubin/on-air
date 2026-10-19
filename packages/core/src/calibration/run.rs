//! One calibration run, end to end: drive the transport with the existing
//! senders, play the probe on the existing PCM bus, listen, measure.
//!
//! # How the transports are driven
//!
//! The run activates the target output through the product's own
//! activation path ([`crate::session::activate`]) — the same Sonos SOAP +
//! radio-stream sender, AirPlay (OwnTone/pyatv) sender, Bluetooth `pacat`/
//! cpal sink and local/Pulse sink a user gets from `POST
//! /api/outputs/active` — and publishes the probe as ordinary
//! [`PcmChunk`]s on the pipeline PCM bus. Every sender and the
//! per-receiver stream (`/stream/{nonce}/audio.wav`) therefore carries the
//! probe exactly as it carries captured audio. There is no second audio
//! path, and none of the senders know a measurement is happening.
//!
//! The probe publisher must be the bus's **only** producer while it runs
//! (a second producer would interleave two timelines into the receivers),
//! so the run suspends the user's input session for the duration and puts
//! it back afterwards.
//!
//! # Which transports run headlessly
//!
//! All four the product supports are driven the same way, and all four run
//! headlessly on the machine that has them — a calibration needs only the
//! core, the speaker and a microphone, no human interaction:
//!
//! - **Sonos**: LAN SOAP control + the receiver pulls the radio WAV.
//!   Headless. ✓
//! - **Bluetooth**: a paired device played through the OS sink (`pacat` /
//!   cpal). Headless once paired. ✓
//! - **local/Pulse**: `pacat` to a Pulse/PipeWire sink. Headless. ✓
//!   (Refused while the input is "System audio" — the product's own
//!   feedback guard.)
//! - **AirPlay**: headless on Linux, where the sender drives the OwnTone
//!   sidecar or a pyatv helper. **Not drivable on macOS**: there the
//!   product's AirPlay is local-picker only and
//!   [`crate::session::activate`] refuses programmatic activation
//!   (`"macOS AirPlay is local-picker only"`). The run reports that target
//!   as failed with that exact reason; nothing is stubbed.
//!
//! # Mock mode (CI only)
//!
//! With `ON_AIR_MOCK=1` there is no hardware and no acoustic world, so the
//! recorder is synthesized from the probe runs plus a configured ground
//! truth ([`MockLoopback`], injected via `POST
//! /api/mock/calibration/loopback`). Everything else — activation through
//! the real session code, probe publication, windowing, detection,
//! aggregation, confidence, applying and persisting — is the production
//! path. This is the harness's own end-to-end test, in the same spirit as
//! the mock senders and `POST /api/mock/cd`; production mode has no
//! synthetic path at all.

use crate::calibration::detect::{detect_delay, shift_bandlimited};
use crate::calibration::probe::ProbeSpec;
use crate::calibration::record::{is_forbidden_capture_input, Recorder, Recording};
use crate::calibration::{aggregate, RunOutcome};
use crate::dsp::bridge::{f32_to_l16_le_into, TpdfDither};
use crate::state::CoreState;
use crate::timeline::PcmChunk;
use bytes::Bytes;
use on_air_api::{
    CalibrationConfidence, CalibrationRunState, CalibrationRunView, CalibrationStatusResponse,
    CalibrationTarget, CalibrationTargetState, CalibrationTargetStatus, StartCalibrationRequest,
};
use parking_lot::Mutex as SyncMutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Receiver settle time between the sender going live and the first probe:
/// Sonos fetches the radio URI on Play, AirPlay attaches its stream after
/// the session starts. A probe that plays before its consumer attaches is
/// truncated, so we wait.
const SETTLE: Duration = Duration::from_millis(1_500);
/// Recorded after each probe: the longest playout delay we search for,
/// plus margin so a late arrival is fully inside its window.
const RUN_MARGIN: Duration = Duration::from_millis(250);
/// The capture window starts this long before the probe leaves the bus, so
/// a tiny (or jitter-negative) apparent delay still lands inside it.
const PRE_ROLL: Duration = Duration::from_millis(50);
/// Published chunk size: the processing task's quantum, so the probe looks
/// exactly like captured audio on the bus.
const CHUNK_FRAMES: usize = 1024;
/// Wall-clock allowance for one transport to go live.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest playout delay the search covers per transport when the request
/// does not choose: AirPlay buffers ~2 s, Sonos an unmeasured ~1 s,
/// Bluetooth ~150 ms, local ~50 ms — the window is generous beyond each.
pub fn default_max_delay_ms(transport: &str) -> u32 {
    match transport {
        "airplay" => 5_000,
        "sonos" => 4_000,
        "bluetooth" => 2_000,
        "local" => 1_500,
        _ => 2_000,
    }
}

/// Mock mode's configured acoustic world: the ground truth the synthetic
/// loopback places behind the probe. CI sets it; production never has one.
/// Every field is optional in `POST /api/mock/calibration/loopback`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MockLoopback {
    /// Where the probe arrives, in microseconds (may be fractional in
    /// sample terms; the placement is band-limited).
    pub delay_us: f64,
    /// Additive white noise RMS added to the capture.
    pub noise_rms: f64,
    /// Linear gain applied to the arrival.
    pub gain: f64,
    /// Run the arrival through a linear-phase FIR standing in for speaker
    /// colouration. Its (known) group delay is part of the ground truth.
    pub colouration: bool,
    pub capture_rate_hz: u32,
    /// Per-block arrival jitter applied to the recording's timestamps, to
    /// exercise the sample-index fit end to end.
    pub jitter_us: u64,
}

impl Default for MockLoopback {
    fn default() -> Self {
        MockLoopback {
            delay_us: 150_253.0,
            noise_rms: 0.0,
            gain: 1.0,
            colouration: false,
            capture_rate_hz: 48_000,
            jitter_us: 0,
        }
    }
}

/// Why `POST /api/sync/calibrate` refused to start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("a calibration run is already in progress")]
    Busy,
    #[error("{0}")]
    Invalid(String),
}

struct Runs {
    active: Option<CalibrationRunView>,
    last: Option<CalibrationRunView>,
}

/// Process-wide calibration state: at most one run at a time, its progress
/// for `GET /api/sync/calibration`, and (mock mode only) the CI world.
pub struct CalibrationController {
    runs: SyncMutex<Runs>,
    /// One run at a time. Owned locking lets the run task hold the guard
    /// past the request that started it.
    gate: Arc<Mutex<()>>,
    next_run_id: AtomicU64,
    mock_loopback: SyncMutex<Option<MockLoopback>>,
}

impl CalibrationController {
    pub fn new() -> Self {
        CalibrationController {
            runs: SyncMutex::new(Runs {
                active: None,
                last: None,
            }),
            gate: Arc::new(Mutex::new(())),
            next_run_id: AtomicU64::new(1),
            mock_loopback: SyncMutex::new(None),
        }
    }

    pub fn status(&self) -> CalibrationStatusResponse {
        let runs = self.runs.lock();
        CalibrationStatusResponse {
            active: runs.active.clone(),
            last: runs.last.clone(),
        }
    }

    /// Mock mode only: configure the synthetic acoustic world.
    pub fn set_mock_loopback(&self, world: Option<MockLoopback>) {
        *self.mock_loopback.lock() = world;
    }

    pub fn mock_loopback(&self) -> Option<MockLoopback> {
        *self.mock_loopback.lock()
    }
}

impl Default for CalibrationController {
    fn default() -> Self {
        Self::new()
    }
}

/// A validated request, ready to run.
struct Resolved {
    targets: Vec<CalibrationTarget>,
    capture_input: String,
    runs_per_target: u32,
    max_delay_ms: Option<u32>,
}

impl Resolved {
    fn new(request: &StartCalibrationRequest) -> Result<Self, StartError> {
        if request.targets.is_empty() || request.targets.len() > 16 {
            return Err(StartError::Invalid(
                "one calibration target is required (up to 16)".into(),
            ));
        }
        for target in &request.targets {
            let known = target.transport == crate::sender::local::TRANSPORT
                || on_air_api::transport::ALL.contains(&target.transport.as_str());
            if !known {
                return Err(StartError::Invalid(format!(
                    "unknown transport: {}",
                    target.transport
                )));
            }
            if target.device_id.trim().is_empty() {
                return Err(StartError::Invalid("device_id is required".into()));
            }
        }
        if request.capture_input.trim().is_empty() {
            return Err(StartError::Invalid("capture_input is required".into()));
        }
        if is_forbidden_capture_input(&request.capture_input) {
            return Err(StartError::Invalid(format!(
                "{} is a monitor/loopback source; pick a real microphone",
                request.capture_input
            )));
        }
        let runs_per_target = request.runs_per_target.unwrap_or(3);
        if !(1..=10).contains(&runs_per_target) {
            return Err(StartError::Invalid(
                "runs_per_target must be between 1 and 10".into(),
            ));
        }
        if let Some(ms) = request.max_delay_ms {
            if !(500..=15_000).contains(&ms) {
                return Err(StartError::Invalid(
                    "max_delay_ms must be between 500 and 15000".into(),
                ));
            }
        }
        Ok(Resolved {
            targets: request.targets.clone(),
            capture_input: request.capture_input.clone(),
            runs_per_target,
            max_delay_ms: request.max_delay_ms,
        })
    }

    fn max_delay(&self, transport: &str) -> Duration {
        Duration::from_millis(
            self.max_delay_ms
                .unwrap_or_else(|| default_max_delay_ms(transport)) as u64,
        )
    }
}

/// Start a calibration run. Returns the run as it begins; the run itself
/// continues in the background and is monitored with `GET
/// /api/sync/calibration` (each milestone also broadcasts
/// `WsEvent::CalibrationChanged`).
pub async fn start(
    state: &CoreState,
    request: StartCalibrationRequest,
) -> Result<CalibrationRunView, StartError> {
    let resolved = Resolved::new(&request)?;
    let gate = state
        .calibration
        .gate
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::Busy)?;
    let run_id = state.calibration.next_run_id.fetch_add(1, Ordering::AcqRel);
    let catalog = crate::session::list(state).await;
    let view = CalibrationRunView {
        run_id,
        state: CalibrationRunState::Running,
        targets: resolved
            .targets
            .iter()
            .map(|target| CalibrationTargetStatus {
                transport: target.transport.clone(),
                device_id: target.device_id.clone(),
                device_name: catalog
                    .iter()
                    .find(|output| {
                        output.transport == target.transport && output.id == target.device_id
                    })
                    .map(|output| output.name.clone())
                    .unwrap_or_default(),
                state: CalibrationTargetState::Pending,
                measurement: None,
                error: None,
            })
            .collect(),
        started_at_unix_s: unix_now(),
    };
    {
        let mut runs = state.calibration.runs.lock();
        runs.active = Some(view.clone());
    }
    notify(state, run_id);

    let task_state = state.clone();
    tokio::spawn(async move {
        run_all(task_state, resolved, run_id).await;
        drop(gate);
    });
    Ok(view)
}

/// The run body: every target in order (transports are exclusive, so
/// targets are measured one after another).
async fn run_all(state: CoreState, resolved: Resolved, run_id: u64) {
    let mut any_applied = false;
    for index in 0..resolved.targets.len() {
        update_target(&state, run_id, index, |target| {
            target.state = CalibrationTargetState::Running;
        });
        let transport = resolved.targets[index].transport.clone();
        let device_id = resolved.targets[index].device_id.clone();
        let outcome = measure_target(&state, &resolved, &transport, &device_id).await;
        let (measurement, error) = match outcome {
            Err(error) => (None, Some(error)),
            Ok((runs, rate_hz)) => {
                match aggregate(&transport, &device_id, &runs, rate_hz, unix_now()) {
                    None => (None, Some(no_arrival_message(&runs))),
                    Some(measurement) if measurement.confidence == CalibrationConfidence::Low => {
                        let error = format!(
                            "low confidence ({} of {} runs trustworthy, spread {} ms); \
                             the default is kept — re-run with the mic closer to the speaker",
                            measurement.runs,
                            runs.len(),
                            measurement.spread_us as f64 / 1000.0
                        );
                        (Some(measurement), Some(error))
                    }
                    Some(measurement) => {
                        state.apply_calibration(&measurement);
                        any_applied = true;
                        (Some(measurement), None)
                    }
                }
            }
        };
        update_target(&state, run_id, index, |target| {
            target.state = if error.is_some() {
                CalibrationTargetState::Failed
            } else {
                CalibrationTargetState::Complete
            };
            target.measurement = measurement;
            target.error = error;
        });
    }
    let final_state = if any_applied {
        CalibrationRunState::Complete
    } else {
        CalibrationRunState::Failed
    };
    {
        let mut runs = state.calibration.runs.lock();
        if let Some(mut active) = runs.active.take() {
            active.state = final_state;
            runs.last = Some(active);
        }
    }
    notify(&state, run_id);
}

fn no_arrival_message(runs: &[RunOutcome]) -> String {
    let best = runs.iter().map(|run| run.prominence).fold(0.0f64, f64::max);
    format!(
        "no trustworthy probe arrival in {} run(s) (best peak/sidelobe {:.1}, need {:.0}); \
         is the microphone hearing the speaker?",
        runs.len(),
        best,
        crate::calibration::PROMINENCE_USABLE
    )
}

/// Measure one output: drive it, play `runs_per_target` probes, detect
/// each arrival. Returns the per-run outcomes and the capture rate they
/// were measured at.
async fn measure_target(
    state: &CoreState,
    resolved: &Resolved,
    transport: &str,
    device_id: &str,
) -> Result<(Vec<RunOutcome>, u32), String> {
    let spec = ProbeSpec::default();
    let max_delay = resolved.max_delay(transport);

    // The probe publisher must be the PCM bus's only producer: suspend the
    // user's input for the duration and put it back afterwards.
    let suspended = state.input().active_name();
    if suspended.is_some() {
        state.input().stop().await;
    }
    let outcome = drive_and_record(state, resolved, transport, device_id, &spec, max_delay).await;
    if let Some(name) = suspended {
        if let Err(error) = state.input().activate(&name).await {
            eprintln!("could not restore the {name} input after calibration: {error}");
        }
    }
    outcome
}

async fn drive_and_record(
    state: &CoreState,
    resolved: &Resolved,
    transport: &str,
    device_id: &str,
    spec: &ProbeSpec,
    max_delay: Duration,
) -> Result<(Vec<RunOutcome>, u32), String> {
    // The microphone first: its timeline must span every probe.
    let mock_world = state.calibration.mock_loopback();
    let recorder = if state.mock {
        Recorder::start_mock(mock_world.unwrap_or_default().capture_rate_hz)
    } else {
        let capture_input = resolved.capture_input.clone();
        tokio::task::spawn_blocking(move || Recorder::start(&capture_input))
            .await
            .map_err(|_| "the recorder stopped unexpectedly".to_string())??
    };
    let capture_rate = recorder.rate_hz();

    // The transport, through the product's own activation path.
    let drive = drive_probes(
        state,
        resolved,
        transport,
        device_id,
        spec,
        max_delay,
        capture_rate,
    )
    .await;
    let _ = state.output().deactivate().await;
    let probe_runs = drive?;

    // Mock mode: no hardware, so the acoustic world is synthesized around
    // the planned timeline — the same ground truth the detector tests use.
    if state.mock {
        let world = mock_world.ok_or(
            "mock mode has no acoustic world configured (POST /api/mock/calibration/loopback)",
        )?;
        synthesize_loopback(&recorder, &probe_runs, capture_rate, max_delay, &world);
    }

    let recording = tokio::task::spawn_blocking(move || recorder.finish())
        .await
        .map_err(|_| "the recorder stopped unexpectedly".to_string())?;
    let outcomes = tokio::task::spawn_blocking(move || {
        probe_runs
            .iter()
            .map(|run| detect_one(&recording, run, capture_rate, max_delay))
            .collect::<Result<Vec<_>, String>>()
    })
    .await
    .map_err(|_| "detection stopped unexpectedly".to_string())??;
    Ok((outcomes, capture_rate))
}

/// One probe run: when its first frame left the bus and the reference to
/// correlate the capture against (rendered at the capture rate).
struct ProbeRun {
    t0: Instant,
    reference: Vec<f32>,
}

/// Activate the output, wait for it, publish every probe on the bus at
/// real-time pace and remember each run's bus time.
async fn drive_probes(
    state: &CoreState,
    resolved: &Resolved,
    transport: &str,
    device_id: &str,
    spec: &ProbeSpec,
    max_delay: Duration,
    capture_rate: u32,
) -> Result<Vec<ProbeRun>, String> {
    crate::session::activate(state, transport, device_id)
        .await
        .map_err(|error| error.to_string())?;
    wait_live(state).await?;
    if !state.mock {
        tokio::time::sleep(SETTLE).await;
    }

    let bus_rate = state.input_sample_rate_hz();
    let probe_bus = spec.render(bus_rate);
    let reference = spec.render(capture_rate);
    let gap_frames =
        chunk_aligned(((max_delay + RUN_MARGIN).as_secs_f64() * f64::from(bus_rate)) as usize);
    let window = probe_window_frames(reference.len(), max_delay, capture_rate);

    // One continuous stream: probe, then silence for the arrival window
    // and margin, then the next probe. Contiguous `first_frame`s, exactly
    // like the processing task publishes, and every probe starts at a
    // chunk boundary so its reference instant is unambiguous.
    let mut sequence: Vec<f32> = Vec::new();
    let mut marks: Vec<(usize, usize)> = Vec::new(); // (frame offset, run index)
    for run in 0..resolved.runs_per_target {
        if run > 0 {
            sequence.resize(sequence.len() + gap_frames, 0.0);
        }
        marks.push((sequence.len(), run as usize));
        sequence.extend_from_slice(&probe_bus);
        sequence.resize(chunk_aligned(sequence.len()), 0.0);
    }
    sequence.resize(sequence.len() + gap_frames, 0.0);

    let mut dither = TpdfDither::for_stream(bus_rate, bus_rate, 2);
    let mut first_frame = 0u64;
    let mut probe_runs = Vec::new();
    let mut next_mark = 0usize;
    let published_at = Instant::now();
    for (chunk_index, chunk) in sequence.chunks(CHUNK_FRAMES).enumerate() {
        let frame = chunk_index * CHUNK_FRAMES;
        if !state.mock {
            let due = published_at + Duration::from_secs_f64(frame as f64 / f64::from(bus_rate));
            tokio::time::sleep_until(tokio::time::Instant::from_std(due)).await;
        }
        if next_mark < marks.len() && frame == marks[next_mark].0 {
            let run = marks[next_mark].1;
            let t0 = if state.mock {
                // No hardware consumes the bus in mock mode: publish the
                // whole stream at once and plan the timeline in sample
                // space, with a deliberate sub-sample fraction so the
                // window-fraction correction is exercised.
                let planned =
                    PRE_ROLL.as_secs_f64() * f64::from(capture_rate) + run as f64 * window + 0.37;
                published_at + Duration::from_secs_f64(planned / f64::from(capture_rate))
            } else {
                // The probe's first frame is chunk-aligned: its pipeline
                // timeline time is its chunk's publish instant.
                Instant::now()
            };
            probe_runs.push(ProbeRun {
                t0,
                reference: reference.clone(),
            });
            next_mark += 1;
        }
        // Interleaved stereo L16: the probe is identical on both channels.
        let mut stereo = Vec::with_capacity(chunk.len() * 2);
        for &sample in chunk {
            stereo.push(sample);
            stereo.push(sample);
        }
        let mut pcm = Vec::new();
        f32_to_l16_le_into(&stereo, &mut dither, &mut pcm);
        let _ = state.audio_tx.send(PcmChunk {
            first_frame,
            captured_at: Instant::now(),
            pcm: Bytes::from(pcm),
        });
        first_frame += chunk.len() as u64;
    }
    Ok(probe_runs)
}

fn chunk_aligned(frames: usize) -> usize {
    frames.div_ceil(CHUNK_FRAMES) * CHUNK_FRAMES
}

/// The capture samples one run's search window covers: pre-roll, the
/// probe, and the longest delay we look for.
fn probe_window_frames(reference_len: usize, max_delay: Duration, capture_rate: u32) -> f64 {
    (PRE_ROLL.as_secs_f64() + max_delay.as_secs_f64()) * f64::from(capture_rate)
        + reference_len as f64
        + RUN_MARGIN.as_secs_f64() * f64::from(capture_rate)
}

/// Wait until the sender is live (or say why not).
async fn wait_live(state: &CoreState) -> Result<(), String> {
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        let snapshot = state.output().snapshot();
        match snapshot.phase {
            on_air_api::OutputPhase::Live => return Ok(()),
            on_air_api::OutputPhase::Failed => {
                return Err(snapshot
                    .error
                    .unwrap_or_else(|| "the output failed to start".to_string()))
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err("the transport did not start within 30 s".to_string());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Detect one run's arrival inside its window of the recording.
fn detect_one(
    recording: &Recording,
    run: &ProbeRun,
    capture_rate: u32,
    max_delay: Duration,
) -> Result<RunOutcome, String> {
    let rate = f64::from(capture_rate);
    let s0 = recording
        .index_of(run.t0)
        .ok_or_else(|| "the recording has no timeline".to_string())?;
    let pre = PRE_ROLL.as_secs_f64() * rate;
    let window_start = s0 - pre;
    let w0 = window_start.floor().max(0.0);
    let fraction = window_start - w0;
    let window_len = probe_window_frames(run.reference.len(), max_delay, capture_rate) as usize;
    let mono = recording.mono();
    let start = w0 as usize;
    if start >= mono.len() {
        return Err("the recording ended before the probe".to_string());
    }
    let end = (start + window_len).min(mono.len());
    let window = &mono[start..end];

    // The reference is pre-rolled by the same amount as the window, so the
    // correlation lag counts from the window start.
    let pre_samples = pre.round() as usize;
    let mut reference = vec![0.0f32; pre_samples];
    reference.extend_from_slice(&run.reference);
    let detection = detect_delay(
        &reference,
        window,
        capture_rate,
        max_delay + PRE_ROLL + Duration::from_millis(10),
    )
    .map_err(|error| error.to_string())?;

    // The window may start at a fractional sample: the probe's energy sits
    // `fraction` of a sample further in, and that belongs to the delay.
    let delay_samples = detection.delay_samples - fraction;
    Ok(RunOutcome {
        delay_us: (delay_samples * 1_000_000.0 / rate).round() as i64,
        prominence: detection.prominence,
        correlation: detection.correlation,
        samples: window.len() as u64,
    })
}

/// Mock mode: place the configured arrival behind each planned probe run.
/// Band-limited placement (sinc shift) keeps the synthetic world an honest
/// sampled signal; the FIR's group delay is part of the expected result.
fn synthesize_loopback(
    recorder: &Recorder,
    runs: &[ProbeRun],
    capture_rate: u32,
    max_delay: Duration,
    world: &MockLoopback,
) {
    let rate = f64::from(capture_rate);
    let window = probe_window_frames(
        runs.first().map(|run| run.reference.len()).unwrap_or(0),
        max_delay,
        capture_rate,
    );
    let total = (planned_start(runs.len(), window, capture_rate) + rate) as usize;
    let mut capture = vec![0.0f32; total];

    let colouration: Vec<f32> = if world.colouration {
        vec![0.1, -0.25, 0.6, 1.2, 0.6, -0.25, 0.1]
    } else {
        vec![1.0]
    };
    let group_delay_samples = (colouration.len() - 1) as f64 / 2.0;

    for (index, run) in runs.iter().enumerate() {
        let s0 = planned_start(index, window, capture_rate);
        // Place at the exact fractional index: the arrival sits at
        // s0 + delay, samples and fractions alike.
        let placed_at = s0.floor();
        let fraction = s0 - placed_at;
        let delay_samples = world.delay_us * rate / 1_000_000.0 + group_delay_samples + fraction;
        let shifted = shift_bandlimited(&run.reference, delay_samples);
        let placed_at = placed_at as usize;
        for (i, sample) in shifted.iter().enumerate() {
            if placed_at + i < capture.len() {
                capture[placed_at + i] += *sample * world.gain as f32;
            }
        }
    }
    // Colouration, then noise, exactly as a room and speaker would.
    if world.colouration {
        let half = colouration.len() / 2;
        let input = capture.clone();
        for i in half..input.len().saturating_sub(half) {
            let mut acc = 0.0;
            for (k, tap) in colouration.iter().enumerate() {
                acc += tap * input[i + k - half];
            }
            capture[i] = acc;
        }
    }
    if world.noise_rms > 0.0 {
        let mut seed = 0x243F_6A88_85A3_08D3u64;
        for sample in &mut capture {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            // uniform ∈ [-0.5, 0.5]; scaled so the noise RMS is exactly
            // `noise_rms`.
            let uniform = (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            *sample += (uniform * world.noise_rms * 12.0f64.sqrt()) as f32;
        }
    }

    // Deliver the synthetic capture in timestamped blocks with per-block
    // arrival jitter, so the run's timeline fit is exercised for real.
    const BLOCK: usize = 512;
    let origin = runs[0]
        .t0
        .checked_sub(Duration::from_secs_f64(
            PRE_ROLL.as_secs_f64() + 0.37 / rate,
        ))
        .unwrap_or_else(Instant::now);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for (index, block) in capture.chunks(BLOCK).enumerate() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let jitter =
            ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 2.0 * world.jitter_us as f64
                / 1_000_000.0;
        let end = (index * BLOCK + block.len()) as f64;
        let at = origin + Duration::from_secs_f64(end / rate + jitter.max(-end / rate));
        recorder.push_mock(at, block.to_vec());
    }
}

/// A run's planned probe start in mock sample space: pre-roll plus one
/// window per earlier run, with a deliberate sub-sample fraction so the
/// window-fraction correction is exercised. Must match the planning in
/// [`drive_probes`] exactly — the mock timeline and the detector's windows
/// meet here.
fn planned_start(index: usize, window: f64, capture_rate: u32) -> f64 {
    PRE_ROLL.as_secs_f64() * f64::from(capture_rate) + index as f64 * window + 0.37
}

fn update_target(
    state: &CoreState,
    run_id: u64,
    index: usize,
    update: impl FnOnce(&mut CalibrationTargetStatus),
) {
    {
        let mut runs = state.calibration.runs.lock();
        if let Some(active) = runs.active.as_mut() {
            if active.run_id == run_id {
                if let Some(target) = active.targets.get_mut(index) {
                    update(target);
                }
            }
        }
    }
    notify(state, run_id);
}

fn notify(state: &CoreState, run_id: u64) {
    let _ = state
        .ws_tx
        .send(on_air_api::WsEvent::CalibrationChanged { run_id });
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
