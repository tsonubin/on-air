# M1 — Linux Audio Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the real audio path on Linux end to end: capture from a PipeWire monitor source, run it through EQ and resampling, stream it to a real Sonos speaker over UPnP/SOAP, and control all of it through `packages/core`'s HTTP/WS API.

**Architecture:** A real-time `cpal` capture callback pushes PCM into a lock-free ring buffer; a dedicated OS thread drains it, applies biquad EQ and `rubato` resampling, and republishes processed PCM on a `tokio::broadcast` channel. That channel feeds both a chunked `/stream/audio.wav` HTTP endpoint (which a `SonosSender` points the speaker at via UPnP `SetAVTransportURI`/`Play`) and nothing else this milestone. `CoreState` holds `Option<Box<dyn AudioSender>>` — activating any sender always stops whatever was active first, enforcing the cross-transport exclusivity rule with only one transport implemented.

**Tech Stack:** Rust, `axum` 0.7 (+`ws` feature), `cpal` 0.18.2, `ringbuf` 0.5.1, `rubato` 5.0.0 + `audioadapter-buffers` 5.2.0, `async-trait` 0.1.92, `tokio-stream` 0.1.19, `reqwest` 0.12, `bytes` 1.

**Spec:** [docs/superpowers/specs/2026-08-27-on-air-m1-linux-audio-path-design.md](../specs/2026-08-27-on-air-m1-linux-audio-path-design.md) (and the parent [2026-08-26-on-air-design.md](../specs/2026-08-26-on-air-design.md))

## Global Constraints

- Scope is `packages/core` only. No `apps/desktop` or `apps/mobile` changes this milestone — GUI wiring is M2, mobile is M3.
- `DEFAULT_PORT = 47990` (existing, `packages/core/src/lib.rs`) — the audio stream endpoint and control API share this port.
- The pipeline is **mono internally** for M1: capture downmixes any device channel count to mono before entering the ring buffer. Stereo is out of scope for M1 (not mandated by the spec, and doubling per-channel biquad/resampler state is unwarranted scope for "prove the path").
- 5-band graphic EQ: center frequencies `[60.0, 250.0, 1000.0, 4000.0, 12000.0]` Hz, gain range `-12.0..=12.0` dB, fixed Q = 1.0 per band (RBJ peaking biquad).
- Volume is **native Sonos volume** via `RenderingControl::SetVolume` — no software gain stage in the PCM path. `AudioSender::set_volume` is per-sender by design.
- Target sample rate defaults to 44100 Hz. Changing it via the API applies on the *next* capture/output start, not live mid-stream (the `rubato` resampler's ratio is fixed at construction).
- EQ gain changes DO apply live mid-stream (the processing thread re-reads shared gain state every chunk).
- Sonos discovery: SSDP M-SEARCH to `239.255.255.250:1900`, `ST: urn:schemas-upnp-org:device:ZonePlayer:1`; discovered devices expire after a 120s TTL if not re-confirmed by a periodic 30s re-search.
- Sonos control URLs are the fixed conventional paths `/MediaRenderer/AVTransport/Control` and `/MediaRenderer/RenderingControl/Control` relative to the LOCATION header's host:port — not parsed from the device description's `serviceList` XML. This is a documented simplification consistent with "hand-rolled, no generic UPnP client."
- Audio delivery: `GET /stream/audio.wav`, `Content-Type: audio/L16;rate=<target_sample_rate>;channels=1`, raw 16-bit signed little-endian PCM chunked over HTTP. Despite the `.wav` suffix (kept because that's what the approved spec's REST table names), this is **not** a RIFF/WAV container — `audio/L16` is the correct, simpler way to serve a live/unbounded PCM stream, and Sonos identifies stream type from `Content-Type`, not the URL suffix.
- `AudioSender` is an `async_trait` trait (`start`/`stop`/`set_volume`/`name`), object-safe, `Send + Sync`. `CoreState.active_sender: Arc<tokio::sync::Mutex<Option<Box<dyn AudioSender>>>>`. Activating a new sender always calls `.stop()` on the current one first.
- Dropping a `cpal::Stream` blocks the calling thread until its internal worker thread joins (verified against cpal 0.18.2's ALSA backend source). Any code path that stops capture from an async handler MUST do so inside `tokio::task::spawn_blocking`.
- Library versions below were verified by actually compiling against them (not assumed from possibly-stale docs) — use exactly these:
  - `cpal = "0.18.2"` (default features — ALSA backend on Linux, which reaches PipeWire's monitor source through PipeWire's ALSA-compatibility layer)
  - `ringbuf = "0.5.1"`
  - `rubato = "5.0.0"` + `audioadapter-buffers = "5.2.0"`
  - `async-trait = "0.1.92"`
  - `tokio-stream = { version = "0.1.19", features = ["sync"] }`
  - `bytes = "1"`
  - `reqwest = { version = "0.12", features = ["json", "stream"] }` (moved from `dev-dependencies` to `dependencies` — needed at runtime for SOAP calls, not just tests)
  - `axum = { version = "0.7", features = ["ws"] }` (the `ws` feature is non-default and must be added explicitly)
  - `tokio-tungstenite = "0.30.0"` (dev-dependency, WebSocket test client only)

---

### Task 1: DSP — 5-band graphic EQ

**Files:**
- Create: `packages/core/src/dsp/mod.rs`
- Create: `packages/core/src/dsp/eq.rs`
- Modify: `packages/core/src/lib.rs` (add `pub mod dsp;`)
- Test: `packages/core/tests/dsp_eq.rs`

**Interfaces:**
- Produces: `dsp::eq::GraphicEq::new(sample_rate_hz: f32) -> GraphicEq`, `GraphicEq::set_gains_db(&mut self, gains_db: [f32; 5])`, `GraphicEq::gains_db(&self) -> [f32; 5]`, `GraphicEq::process(&mut self, samples: &mut [f32])` (in-place, mono). `dsp::eq::EQ_BAND_CENTERS_HZ: [f32; 5]`, `dsp::eq::EQ_GAIN_RANGE_DB: (f32, f32)`.

- [ ] **Step 1: Write the failing tests**

```rust
// packages/core/tests/dsp_eq.rs
use on_air_core::dsp::eq::{GraphicEq, EQ_BAND_CENTERS_HZ, EQ_GAIN_RANGE_DB};

#[test]
fn zero_gain_is_exact_passthrough() {
    let mut eq = GraphicEq::new(44100.0);
    let input: Vec<f32> = (0..2000)
        .map(|i| (i as f32 * 0.017).sin() * 0.6)
        .collect();
    let mut samples = input.clone();
    eq.process(&mut samples);
    for (a, b) in input.iter().zip(samples.iter()) {
        assert!((a - b).abs() < 1e-4, "expected passthrough, got {a} vs {b}");
    }
}

#[test]
fn set_gains_db_clamps_to_range() {
    let mut eq = GraphicEq::new(44100.0);
    eq.set_gains_db([20.0, -20.0, 0.0, 0.0, 0.0]);
    let gains = eq.gains_db();
    assert_eq!(gains[0], EQ_GAIN_RANGE_DB.1);
    assert_eq!(gains[1], EQ_GAIN_RANGE_DB.0);
}

#[test]
fn boosting_a_band_amplifies_its_center_frequency() {
    let sample_rate = 44100.0;
    let band_index = 2; // 1000 Hz
    let freq = EQ_BAND_CENTERS_HZ[band_index];

    let mut gains = [0.0; 5];
    gains[band_index] = 12.0;

    let n = 4000;
    let sine: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
        .collect();

    let mut boosted = sine.clone();
    let mut eq = GraphicEq::new(sample_rate);
    eq.set_gains_db(gains);
    eq.process(&mut boosted);

    // skip the filter's settling transient, compare steady-state RMS
    let settle = n / 4;
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
    let ratio = rms(&boosted[settle..]) / rms(&sine[settle..]);

    // +12dB amplitude ratio is 10^(12/20) ~= 3.98
    assert!(ratio > 3.0 && ratio < 4.5, "ratio was {ratio}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p on-air-core --test dsp_eq`
Expected: FAIL to compile — `on_air_core::dsp` does not exist yet.

- [ ] **Step 3: Implement the biquad graphic EQ**

```rust
// packages/core/src/dsp/mod.rs
pub mod eq;
```

```rust
// packages/core/src/dsp/eq.rs
pub const EQ_BAND_CENTERS_HZ: [f32; 5] = [60.0, 250.0, 1000.0, 4000.0, 12000.0];
pub const EQ_GAIN_RANGE_DB: (f32, f32) = (-12.0, 12.0);
const Q: f32 = 1.0;

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

fn peaking_eq_coeffs(sample_rate_hz: f32, freq_hz: f32, gain_db: f32, q: f32) -> BiquadCoeffs {
    let a = 10f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate_hz;
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

fn process_one(coeffs: &BiquadCoeffs, state: &mut BiquadState, x0: f32) -> f32 {
    let y0 = coeffs.b0 * x0 + coeffs.b1 * state.x1 + coeffs.b2 * state.x2
        - coeffs.a1 * state.y1
        - coeffs.a2 * state.y2;
    state.x2 = state.x1;
    state.x1 = x0;
    state.y2 = state.y1;
    state.y1 = y0;
    y0
}

pub struct GraphicEq {
    sample_rate_hz: f32,
    gains_db: [f32; 5],
    coeffs: [BiquadCoeffs; 5],
    state: [BiquadState; 5],
}

impl GraphicEq {
    pub fn new(sample_rate_hz: f32) -> Self {
        let gains_db = [0.0; 5];
        let coeffs = Self::compute_coeffs(sample_rate_hz, &gains_db);
        GraphicEq {
            sample_rate_hz,
            gains_db,
            coeffs,
            state: [BiquadState::default(); 5],
        }
    }

    fn compute_coeffs(sample_rate_hz: f32, gains_db: &[f32; 5]) -> [BiquadCoeffs; 5] {
        std::array::from_fn(|i| {
            peaking_eq_coeffs(sample_rate_hz, EQ_BAND_CENTERS_HZ[i], gains_db[i], Q)
        })
    }

    pub fn set_gains_db(&mut self, gains_db: [f32; 5]) {
        let clamped = gains_db.map(|g| g.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1));
        self.gains_db = clamped;
        self.coeffs = Self::compute_coeffs(self.sample_rate_hz, &clamped);
    }

    pub fn gains_db(&self) -> [f32; 5] {
        self.gains_db
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let mut x = *sample;
            for band in 0..5 {
                x = process_one(&self.coeffs[band], &mut self.state[band], x);
            }
            *sample = x;
        }
    }
}
```

- [ ] **Step 4: Wire `dsp` into the crate root**

```rust
// packages/core/src/lib.rs — add near the top, with the other `pub const`/`pub struct` items
pub mod dsp;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test dsp_eq`
Expected: PASS (3/3)

- [ ] **Step 6: Commit**

```bash
git add packages/core/src/dsp packages/core/src/lib.rs packages/core/tests/dsp_eq.rs
git commit -m "feat(core): add 5-band graphic EQ (biquad peaking filters)"
```

---

### Task 2: DSP — mono resampler wrapper (rubato)

**Files:**
- Create: `packages/core/src/dsp/resample.rs`
- Modify: `packages/core/src/dsp/mod.rs` (add `pub mod resample;`)
- Modify: `packages/core/Cargo.toml` (add `rubato`, `audioadapter-buffers`)
- Test: `packages/core/tests/dsp_resample.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `dsp::resample::MonoResampler::new(input_rate_hz: u32, output_rate_hz: u32) -> MonoResampler`, `MonoResampler::input_frames_next(&self) -> usize`, `MonoResampler::process(&mut self, input: &[f32]) -> Vec<f32>` (panics if `input.len() != input_frames_next()` — callers always query it first).

- [ ] **Step 1: Add dependencies**

```toml
# packages/core/Cargo.toml — add under [dependencies]
rubato = "5.0.0"
audioadapter-buffers = "5.2.0"
```

- [ ] **Step 2: Write the failing test**

```rust
// packages/core/tests/dsp_resample.rs
use on_air_core::dsp::resample::MonoResampler;

#[test]
fn upsamples_44100_to_48000_with_correct_ratio() {
    let mut r = MonoResampler::new(44100, 48000);
    let n = r.input_frames_next();
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
    let out = r.process(&input);

    assert!(!out.is_empty());
    let ratio = out.len() as f64 / n as f64;
    assert!((ratio - 48000.0 / 44100.0).abs() < 0.05, "ratio was {ratio}");
}

#[test]
fn identity_ratio_is_near_lossless() {
    let mut r = MonoResampler::new(44100, 44100);
    let n = r.input_frames_next();
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
    let out = r.process(&input);

    assert_eq!(out.len(), input.len());
    let max_diff = input
        .iter()
        .zip(out.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_diff < 0.02, "max_diff was {max_diff}");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test dsp_resample`
Expected: FAIL to compile — `on_air_core::dsp::resample` does not exist yet.

- [ ] **Step 4: Implement the resampler wrapper**

```rust
// packages/core/src/dsp/resample.rs
use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const CHUNK_SIZE: usize = 1024;

pub struct MonoResampler {
    inner: Async<f32>,
}

impl MonoResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32) -> Self {
        let ratio = output_rate_hz as f64 / input_rate_hz as f64;
        let inner = Async::<f32>::new_poly(
            ratio,
            1.1,
            PolynomialDegree::Cubic,
            CHUNK_SIZE,
            1,
            FixedAsync::Input,
        )
        .expect("valid resample ratio");
        MonoResampler { inner }
    }

    pub fn input_frames_next(&self) -> usize {
        self.inner.input_frames_next()
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        assert_eq!(
            input.len(),
            self.inner.input_frames_next(),
            "MonoResampler::process requires exactly input_frames_next() samples"
        );
        let input_data = vec![input.to_vec()];
        let out_capacity = self.inner.output_frames_max();
        let mut output_data = vec![vec![0.0f32; out_capacity]];

        let in_adapter = SequentialSliceOfVecs::new(&input_data, 1, input.len())
            .expect("valid input adapter shape");
        let mut out_adapter = SequentialSliceOfVecs::new_mut(&mut output_data, 1, out_capacity)
            .expect("valid output adapter shape");

        let (_frames_read, frames_written) = self
            .inner
            .process_into_buffer(&in_adapter, &mut out_adapter, None)
            .expect("resample succeeds for a full, correctly-sized chunk");

        output_data.remove(0).into_iter().take(frames_written).collect()
    }
}
```

- [ ] **Step 5: Wire `resample` into `dsp`**

```rust
// packages/core/src/dsp/mod.rs
pub mod eq;
pub mod resample;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test dsp_resample`
Expected: PASS (2/2)

- [ ] **Step 7: Commit**

```bash
git add packages/core/src/dsp packages/core/Cargo.toml packages/core/Cargo.lock packages/core/tests/dsp_resample.rs
git commit -m "feat(core): add mono resampler wrapper around rubato"
```

---

### Task 3: `CoreState`, `AudioSender` trait, `NullSender`, exclusivity

**Files:**
- Create: `packages/core/src/sender/mod.rs`
- Create: `packages/core/src/state.rs`
- Modify: `packages/core/src/lib.rs` (`pub mod sender; pub mod state;`, `build_router` signature change, `serve`/`serve_on` construct default state)
- Modify: `packages/core/tests/status_router.rs` (pass `CoreState` into `build_router`)
- Modify: `packages/core/Cargo.toml` (add `async-trait`, `bytes`)
- Test: `packages/core/tests/sender_exclusivity.rs`

**Interfaces:**
- Produces: `sender::{AudioSender, SenderError, NullSender}`. `AudioSender` is `#[async_trait::async_trait] pub trait AudioSender: Send + Sync { async fn start(&mut self) -> Result<(), SenderError>; async fn stop(&mut self) -> Result<(), SenderError>; async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError>; fn name(&self) -> &str; }`. `state::CoreState` (Clone) with `active_sender: Arc<tokio::sync::Mutex<Option<Box<dyn AudioSender>>>>`, `eq_gains_db: Arc<std::sync::Mutex<[f32; 5]>>`, `target_sample_rate_hz: Arc<std::sync::Mutex<u32>>`, `audio_tx: tokio::sync::broadcast::Sender<bytes::Bytes>`. `CoreState::new() -> CoreState`, `CoreState::activate_sender(&self, new_sender: Box<dyn AudioSender>) -> Result<(), SenderError>`, `CoreState::deactivate_sender(&self) -> Result<(), SenderError>`.
- `build_router(state: CoreState) -> Router` (was `build_router()`).

- [ ] **Step 1: Add dependencies**

```toml
# packages/core/Cargo.toml — add under [dependencies]
async-trait = "0.1.92"
bytes = "1"
```

- [ ] **Step 2: Write the failing test**

```rust
// packages/core/tests/sender_exclusivity.rs
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
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test sender_exclusivity`
Expected: FAIL to compile — `on_air_core::sender` and `on_air_core::state` do not exist yet.

- [ ] **Step 4: Implement the `AudioSender` trait and `NullSender`**

```rust
// packages/core/src/sender/mod.rs
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct SenderError(pub String);

impl std::fmt::Display for SenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SenderError {}

#[async_trait::async_trait]
pub trait AudioSender: Send + Sync {
    async fn start(&mut self) -> Result<(), SenderError>;
    async fn stop(&mut self) -> Result<(), SenderError>;
    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError>;
    fn name(&self) -> &str;
}

/// Test/CI fake — logs every call instead of touching real hardware or network.
pub struct NullSender {
    name: String,
    log: Arc<Mutex<Vec<String>>>,
}

impl NullSender {
    pub fn new(name: impl Into<String>, log: Arc<Mutex<Vec<String>>>) -> Self {
        NullSender {
            name: name.into(),
            log,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for NullSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:start", self.name));
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.log.lock().await.push(format!("{}:stop", self.name));
        Ok(())
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.log
            .lock()
            .await
            .push(format!("{}:volume:{}", self.name, volume));
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}
```

- [ ] **Step 5: Implement `CoreState`**

```rust
// packages/core/src/state.rs
use crate::sender::{AudioSender, SenderError};
use bytes::Bytes;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{broadcast, Mutex};

#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;

impl CoreState {
    pub fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(64);
        CoreState {
            active_sender: Arc::new(Mutex::new(None)),
            eq_gains_db: Arc::new(StdMutex::new([0.0; 5])),
            target_sample_rate_hz: Arc::new(StdMutex::new(TARGET_SAMPLE_RATE_DEFAULT_HZ)),
            audio_tx,
        }
    }

    pub async fn activate_sender(&self, new_sender: Box<dyn AudioSender>) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            current.stop().await?;
        }
        let mut new_sender = new_sender;
        new_sender.start().await?;
        *guard = Some(new_sender);
        Ok(())
    }

    pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            current.stop().await?;
        }
        Ok(())
    }
}

impl Default for CoreState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 6: Wire `sender`/`state` into the crate root and thread `CoreState` through the router**

```rust
// packages/core/src/lib.rs
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub mod dsp;
pub mod sender;
pub mod state;

use state::CoreState;

// Keep in sync with DEFAULT_PORT in packages/api-types/src/index.ts
pub const DEFAULT_PORT: u16 = 47990;

#[derive(Serialize, PartialEq, Debug)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub fn status() -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }
}

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .with_state(state)
}

async fn status_handler() -> Json<StatusResponse> {
    Json(status())
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, build_router(CoreState::new())).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}
```

- [ ] **Step 7: Update the existing status router test for the new signature**

```rust
// packages/core/tests/status_router.rs — only the router construction line changes
use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn status_route_returns_ok_json() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}
```

- [ ] **Step 8: Run all tests to verify everything passes**

Run: `cargo test -p on-air-core`
Expected: PASS — `dsp_eq`, `dsp_resample`, `sender_exclusivity` (3/3), `status_router`, `serve_integration` all green.

- [ ] **Step 9: Commit**

```bash
git add packages/core/src/sender packages/core/src/state.rs packages/core/src/lib.rs \
        packages/core/tests/status_router.rs packages/core/tests/sender_exclusivity.rs \
        packages/core/Cargo.toml packages/core/Cargo.lock
git commit -m "feat(core): add CoreState, AudioSender trait, NullSender, sender exclusivity"
```

---

### Task 4: Ring buffer + async processing pipeline

**Files:**
- Create: `packages/core/src/pipeline/mod.rs`
- Modify: `packages/core/src/lib.rs` (`pub mod pipeline;`)
- Modify: `packages/core/Cargo.toml` (add `ringbuf`)
- Test: `packages/core/tests/pipeline_processing.rs`

**Interfaces:**
- Consumes: `dsp::eq::GraphicEq` (Task 1), `dsp::resample::MonoResampler` (Task 2).
- Produces: `pipeline::spawn_processing_task(consumer: ringbuf::HeapCons<f32>, input_rate_hz: u32, output_rate_hz: u32, eq_gains_db: Arc<std::sync::Mutex<[f32; 5]>>, audio_tx: broadcast::Sender<Bytes>) -> ProcessingTaskHandle`. `ProcessingTaskHandle::stop(self)` (blocking — join the processing thread; callers on the async side must wrap this in `spawn_blocking`). `pipeline::new_ring_buffer(capacity_frames: usize) -> (ringbuf::HeapProd<f32>, ringbuf::HeapCons<f32>)`.

This task's tests push synthetic frames directly into a ring buffer producer — no `cpal`/audio hardware dependency. Real capture wiring is Task 5.

- [ ] **Step 1: Add dependency**

```toml
# packages/core/Cargo.toml — add under [dependencies]
ringbuf = "0.5.1"
```

- [ ] **Step 2: Write the failing test**

```rust
// packages/core/tests/pipeline_processing.rs
use bytes::Bytes;
use on_air_core::pipeline::{new_ring_buffer, spawn_processing_task};
use ringbuf::traits::Producer;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

#[tokio::test]
async fn processes_synthetic_frames_into_broadcast_pcm() {
    let (mut producer, consumer) = new_ring_buffer(1024 * 8);
    let (audio_tx, mut audio_rx) = broadcast::channel(8);
    let eq_gains_db = Arc::new(Mutex::new([0.0; 5]));

    let handle = spawn_processing_task(consumer, 44100, 44100, eq_gains_db, audio_tx);

    // push more than one resampler chunk's worth of a known sine wave
    let samples: Vec<f32> = (0..1024 * 2)
        .map(|i| (i as f32 * 0.05).sin() * 0.4)
        .collect();
    let mut pushed = 0;
    while pushed < samples.len() {
        pushed += producer.push_slice(&samples[pushed..]);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let chunk: Bytes = tokio::time::timeout(Duration::from_secs(2), audio_rx.recv())
        .await
        .expect("received a chunk before timing out")
        .expect("channel not closed");

    assert!(!chunk.is_empty());
    assert_eq!(chunk.len() % 2, 0, "L16 PCM is 2 bytes per sample");

    tokio::task::spawn_blocking(move || handle.stop())
        .await
        .unwrap();
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test pipeline_processing`
Expected: FAIL to compile — `on_air_core::pipeline` does not exist yet.

- [ ] **Step 4: Implement the ring buffer + processing thread**

```rust
// packages/core/src/pipeline/mod.rs
use crate::dsp::eq::GraphicEq;
use crate::dsp::resample::MonoResampler;
use bytes::Bytes;
use ringbuf::{traits::*, HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

pub fn new_ring_buffer(capacity_frames: usize) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(capacity_frames).split()
}

pub struct ProcessingTaskHandle {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl ProcessingTaskHandle {
    /// Blocking: joins the processing thread. From async code, call this
    /// inside `tokio::task::spawn_blocking`.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn f32_to_le_i16_bytes(samples: &[f32]) -> Bytes {
    let mut buf = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let sample_i16 = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample_i16.to_le_bytes());
    }
    Bytes::from(buf)
}

pub fn spawn_processing_task(
    mut consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<Bytes>,
) -> ProcessingTaskHandle {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();

    let join_handle = std::thread::spawn(move || {
        let mut eq = GraphicEq::new(input_rate_hz as f32);
        let mut resampler = MonoResampler::new(input_rate_hz, output_rate_hz);
        let chunk_frames = resampler.input_frames_next();
        let mut chunk = vec![0.0f32; chunk_frames];

        while !stop_flag_thread.load(Ordering::Relaxed) {
            let mut filled = 0;
            while filled < chunk_frames {
                if stop_flag_thread.load(Ordering::Relaxed) {
                    return;
                }
                filled += consumer.pop_slice(&mut chunk[filled..]);
                if filled < chunk_frames {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }

            eq.set_gains_db(*eq_gains_db.lock().unwrap());
            eq.process(&mut chunk);
            let resampled = resampler.process(&chunk);
            let pcm_bytes = f32_to_le_i16_bytes(&resampled);
            let _ = audio_tx.send(pcm_bytes); // no subscribers is fine
        }
    });

    ProcessingTaskHandle {
        stop_flag,
        join_handle: Some(join_handle),
    }
}
```

- [ ] **Step 5: Wire `pipeline` into the crate root**

```rust
// packages/core/src/lib.rs — add alongside the other pub mod declarations
pub mod pipeline;
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p on-air-core --test pipeline_processing`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add packages/core/src/pipeline packages/core/src/lib.rs \
        packages/core/tests/pipeline_processing.rs packages/core/Cargo.toml packages/core/Cargo.lock
git commit -m "feat(core): add ring buffer + async EQ/resample processing pipeline"
```

---

### Task 5: `/stream/audio.wav` HTTP endpoint

**Files:**
- Create: `packages/core/src/api/mod.rs`
- Create: `packages/core/src/api/stream.rs`
- Modify: `packages/core/src/lib.rs` (`pub mod api;`, add route to `build_router`)
- Modify: `packages/core/Cargo.toml` (add `tokio-stream`)
- Test: `packages/core/tests/stream_endpoint.rs`

**Interfaces:**
- Consumes: `state::CoreState.audio_tx` (Task 3), `state::CoreState.target_sample_rate_hz` (Task 3).
- Produces: `api::stream::stream_audio` (axum handler), registered at `GET /stream/audio.wav`.

- [ ] **Step 1: Add dependencies**

```toml
# packages/core/Cargo.toml — add under [dependencies]
tokio-stream = { version = "0.1.19", features = ["sync"] }
```

The test below reads the response body via `bytes_stream()`, which needs
reqwest's `stream` feature — not enabled yet on the existing M0
dev-dependency entry. `reqwest` is still dev-only at this point in the plan
(it becomes a regular dependency in Task 7, once library code needs it too);
for now just add the feature to the existing entry:

```toml
# packages/core/Cargo.toml — update the existing [dev-dependencies] entry
reqwest = { version = "0.12", features = ["json", "stream"] }
```

- [ ] **Step 2: Write the failing test**

```rust
// packages/core/tests/stream_endpoint.rs
use futures_util::StreamExt;
use on_air_core::state::CoreState;
use tokio::net::TcpListener;

#[tokio::test]
async fn streams_published_pcm_chunks_with_correct_content_type() {
    let state = CoreState::new();
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
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "audio/L16;rate=44100;channels=1"
    );

    let mut stream = response.bytes_stream();
    let first_chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("received a chunk before timing out")
        .expect("stream not closed")
        .expect("chunk read ok");

    assert_eq!(first_chunk, known_chunk);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test stream_endpoint`
Expected: FAIL to compile — `on_air_core::api` does not exist yet, and the test needs `futures-util` as a dev-dependency for `StreamExt`.

```toml
# packages/core/Cargo.toml — add under [dev-dependencies]
futures-util = "0.3"
```

- [ ] **Step 4: Implement the streaming endpoint**

```rust
// packages/core/src/api/mod.rs
pub mod stream;
```

```rust
// packages/core/src/api/stream.rs
use crate::state::CoreState;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use tokio_stream::wrappers::BroadcastStream;

pub async fn stream_audio(State(state): State<CoreState>) -> Response {
    let rx = state.audio_tx.subscribe();
    let sample_rate = *state.target_sample_rate_hz.lock().unwrap();
    let body = Body::from_stream(BroadcastStream::new(rx));

    (
        [
            (
                header::CONTENT_TYPE,
                format!("audio/L16;rate={sample_rate};channels=1"),
            ),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}
```

- [ ] **Step 5: Wire `api` into the crate root and register the route**

```rust
// packages/core/src/lib.rs
pub mod api;
// ... (dsp, sender, state, pipeline stay as-is)

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .with_state(state)
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p on-air-core --test stream_endpoint`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add packages/core/src/api packages/core/src/lib.rs \
        packages/core/tests/stream_endpoint.rs packages/core/Cargo.toml packages/core/Cargo.lock
git commit -m "feat(core): add /stream/audio.wav chunked L16 PCM endpoint"
```

---

### Task 6: `cpal` capture wiring + input listing/activation API

**Files:**
- Create: `packages/core/src/pipeline/capture.rs`
- Create: `packages/core/src/api/inputs.rs`
- Modify: `packages/core/src/pipeline/mod.rs` (`pub mod capture;`)
- Modify: `packages/core/src/state.rs` (add `capture` field)
- Modify: `packages/core/src/api/mod.rs` (`pub mod inputs;`)
- Modify: `packages/core/src/lib.rs` (register `/api/inputs`, `/api/inputs/active` routes)
- Modify: `packages/core/Cargo.toml` (add `cpal`, `serde_json` already present)
- Test: `packages/core/tests/capture_devices.rs` (enumeration only — no real stream opened in CI)
- Test: `packages/core/tests/api_inputs.rs`

**Interfaces:**
- Consumes: `pipeline::new_ring_buffer`, `pipeline::spawn_processing_task`, `pipeline::ProcessingTaskHandle` (Task 4).
- Produces: `pipeline::capture::list_input_devices(host: &cpal::Host) -> Result<Vec<InputDeviceInfo>, cpal::Error>`, `pipeline::capture::find_input_device(host: &cpal::Host, name: &str) -> Result<Option<cpal::Device>, cpal::Error>`, `pipeline::capture::start_capture(device: &cpal::Device, producer: HeapProd<f32>) -> Result<cpal::Stream, cpal::Error>`. `pipeline::CaptureHandle { stream: cpal::Stream, processing: ProcessingTaskHandle, device_name: String }`, `CaptureHandle::stop(self)` (blocking). `CoreState.capture: Arc<tokio::sync::Mutex<Option<pipeline::CaptureHandle>>>`. Routes: `GET /api/inputs`, `POST /api/inputs/active`.

- [ ] **Step 1: Add dependency**

```toml
# packages/core/Cargo.toml — add under [dependencies]
cpal = "0.18.2"
```

- [ ] **Step 2: Write the failing enumeration test**

```rust
// packages/core/tests/capture_devices.rs
use on_air_core::pipeline::capture::list_input_devices;

#[test]
fn listing_input_devices_does_not_panic() {
    let host = cpal::default_host();
    // CI/sandboxed environments may have zero input devices — that's fine,
    // this only asserts the enumeration path itself doesn't error or panic.
    let result = list_input_devices(&host);
    assert!(result.is_ok());
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test capture_devices`
Expected: FAIL to compile — `on_air_core::pipeline::capture` does not exist yet.

- [ ] **Step 4: Implement capture wiring**

```rust
// packages/core/src/pipeline/capture.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, InputCallbackInfo, SampleFormat, Stream};
use ringbuf::{traits::Producer, HeapProd};

#[derive(Debug, Clone, PartialEq)]
pub struct InputDeviceInfo {
    pub name: String,
}

pub fn list_input_devices(host: &Host) -> Result<Vec<InputDeviceInfo>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices
        .map(|d| InputDeviceInfo {
            name: d.to_string(),
        })
        .collect())
}

pub fn find_input_device(host: &Host, name: &str) -> Result<Option<Device>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices.into_iter().find(|d| d.to_string() == name))
}

/// Opens the device's default input config and starts pushing captured
/// samples into `producer`, downmixing to mono. Returns the running
/// `Stream` — dropping it stops capture (cpal's Drop impl joins its
/// worker thread, so drop it via `spawn_blocking` from async code).
pub fn start_capture(device: &Device, mut producer: HeapProd<f32>) -> Result<Stream, cpal::Error> {
    let supported = device.default_input_config()?;
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let config = supported.config();

    let err_fn = |err: cpal::Error| {
        eprintln!("capture stream error: {err}");
    };

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &InputCallbackInfo| {
                if channels <= 1 {
                    producer.push_slice(data);
                } else {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    producer.push_slice(&mono);
                }
            },
            err_fn,
            None,
        )?,
        other => {
            return Err(cpal::Error::with_message(
                cpal::ErrorKind::UnsupportedConfig,
                format!("unsupported input sample format: {other:?}"),
            ))
        }
    };

    stream.play()?;
    Ok(stream)
}
```

- [ ] **Step 5: Add `CaptureHandle` and wire `capture` into `pipeline`**

```rust
// packages/core/src/pipeline/mod.rs — add at the end of the existing file
pub mod capture;

pub struct CaptureHandle {
    pub stream: cpal::Stream,
    pub processing: ProcessingTaskHandle,
    pub device_name: String,
}

impl CaptureHandle {
    /// Blocking: stops the capture stream and joins the processing thread.
    /// From async code, call this inside `tokio::task::spawn_blocking`.
    pub fn stop(self) {
        drop(self.stream);
        self.processing.stop();
    }
}
```

- [ ] **Step 6: Add `capture` field to `CoreState`**

```rust
// packages/core/src/state.rs — add to the struct and constructor
#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
}
```

```rust
// in CoreState::new() — add to the struct literal
capture: Arc::new(Mutex::new(None)),
```

- [ ] **Step 7: Implement the REST handlers**

```rust
// packages/core/src/api/inputs.rs
use crate::pipeline::{self, capture};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const RING_BUFFER_CAPACITY_FRAMES: usize = 1024 * 8;

#[derive(Serialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

pub async fn list_inputs() -> Result<Json<InputsResponse>, StatusCode> {
    let host = cpal::default_host();
    let devices = capture::list_input_devices(&host).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(InputsResponse {
        inputs: devices.into_iter().map(|d| d.name).collect(),
    }))
}

#[derive(Deserialize)]
pub struct ActivateInputRequest {
    pub name: String,
}

pub async fn activate_input(
    State(state): State<CoreState>,
    Json(req): Json<ActivateInputRequest>,
) -> Response {
    let host = cpal::default_host();
    let device = match capture::find_input_device(&host, &req.name) {
        Ok(Some(d)) => d,
        Ok(None) => return (StatusCode::NOT_FOUND, "input device not found").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let target_rate = *state.target_sample_rate_hz.lock().unwrap();
    let input_rate = device
        .default_input_config()
        .map(|c| c.sample_rate().0)
        .unwrap_or(target_rate);

    let (producer, consumer) = pipeline::new_ring_buffer(RING_BUFFER_CAPACITY_FRAMES);
    let stream = match capture::start_capture(&device, producer) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let processing = pipeline::spawn_processing_task(
        consumer,
        input_rate,
        target_rate,
        state.eq_gains_db.clone(),
        state.audio_tx.clone(),
    );

    let new_handle = pipeline::CaptureHandle {
        stream,
        processing,
        device_name: req.name.clone(),
    };

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *guard = Some(new_handle);

    StatusCode::NO_CONTENT.into_response()
}

use cpal::traits::DeviceTrait;
```

- [ ] **Step 8: Wire `inputs` into `api` and register routes**

```rust
// packages/core/src/api/mod.rs
pub mod inputs;
pub mod stream;
```

```rust
// packages/core/src/lib.rs
use axum::routing::post;

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .with_state(state)
}
```

- [ ] **Step 9: Write the API test**

```rust
// packages/core/tests/api_inputs.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn list_inputs_returns_ok_json_array() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inputs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["inputs"].is_array());
}

#[tokio::test]
async fn activate_unknown_input_returns_404() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inputs/active")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"definitely-not-a-real-device"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 10: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test capture_devices --test api_inputs`
Expected: PASS (both tests run in CI without real audio hardware since they only enumerate/reject-unknown; a real capture start is exercised in Task 12's manual validation.)

- [ ] **Step 11: Commit**

```bash
git add packages/core/src/pipeline packages/core/src/api packages/core/src/state.rs \
        packages/core/src/lib.rs packages/core/tests/capture_devices.rs \
        packages/core/tests/api_inputs.rs packages/core/Cargo.toml packages/core/Cargo.lock
git commit -m "feat(core): wire cpal capture into pipeline, add /api/inputs endpoints"
```

---

### Task 7: Sonos SSDP discovery + `/api/outputs`

**Files:**
- Create: `packages/core/src/sender/sonos/mod.rs`
- Create: `packages/core/src/sender/sonos/discovery.rs`
- Create: `packages/core/src/api/outputs.rs`
- Modify: `packages/core/src/sender/mod.rs` (`pub mod sonos;`)
- Modify: `packages/core/src/api/mod.rs` (`pub mod outputs;`)
- Modify: `packages/core/src/state.rs` (add `outputs` field, spawn periodic discovery)
- Modify: `packages/core/src/lib.rs` (register `GET /api/outputs`)
- Modify: `packages/core/Cargo.toml` (move `reqwest` from `dev-dependencies` to `dependencies`, add `stream` feature)
- Test: `packages/core/tests/sonos_discovery.rs`

**Interfaces:**
- Produces: `sender::sonos::discovery::{SonosDevice, DeviceRegistry, search_once, fetch_friendly_name}`. `SonosDevice { usn, location, ip, friendly_name }` with `av_transport_control_url(&self) -> String`, `rendering_control_url(&self) -> String`. `DeviceRegistry::new()`, `upsert(&mut self, device: SonosDevice, seen_at: Instant)`, `expire_stale(&mut self, now: Instant, ttl: Duration)`, `list(&self) -> Vec<SonosDevice>`. `CoreState.outputs: Arc<tokio::sync::Mutex<DeviceRegistry>>`. Route: `GET /api/outputs`.

`fetch_friendly_name` (below) takes `&reqwest::Client` in a public library signature, not just in tests — so `reqwest` needs to be a regular dependency starting this task, not a dev-only one.

- [ ] **Step 1: Move `reqwest` to regular dependencies**

```toml
# packages/core/Cargo.toml
[dependencies]
# ... existing entries ...
reqwest = { version = "0.12", features = ["json", "stream"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
futures-util = "0.3"
# (reqwest line removed here — it's a regular dependency now)
```

- [ ] **Step 2: Write the failing tests**

```rust
// packages/core/tests/sonos_discovery.rs
use on_air_core::sender::sonos::discovery::{fetch_friendly_name, search_once, DeviceRegistry, SonosDevice};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[test]
fn device_registry_expires_stale_entries() {
    let mut registry = DeviceRegistry::new();
    let device = SonosDevice {
        usn: "uuid:test-device".into(),
        location: "http://127.0.0.1:1400/xml/device_description.xml".into(),
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        friendly_name: "Test Speaker".into(),
    };
    let t0 = Instant::now();
    registry.upsert(device.clone(), t0);
    assert_eq!(registry.list().len(), 1);

    let later = t0 + Duration::from_secs(200);
    registry.expire_stale(later, Duration::from_secs(120));
    assert_eq!(registry.list().len(), 0);
}

#[test]
fn control_urls_are_derived_from_location() {
    let device = SonosDevice {
        usn: "uuid:test-device".into(),
        location: "http://192.168.1.50:1400/xml/device_description.xml".into(),
        ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
        friendly_name: "Living Room".into(),
    };
    assert_eq!(
        device.av_transport_control_url(),
        "http://192.168.1.50:1400/MediaRenderer/AVTransport/Control"
    );
    assert_eq!(
        device.rendering_control_url(),
        "http://192.168.1.50:1400/MediaRenderer/RenderingControl/Control"
    );
}

#[tokio::test]
async fn search_once_discovers_a_fake_responder() {
    // Fake Sonos device: replies to any UDP datagram with a crafted SSDP response.
    let responder_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let responder_addr = responder_socket.local_addr().unwrap();

    // Fake device_description.xml server.
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http_listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = http_listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let xml = "<root><device><roomName>Living Room</roomName></device></root>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\n\r\n{}",
                xml.len(),
                xml
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        if let Ok((_, src)) = responder_socket.recv_from(&mut buf).await {
            let reply = format!(
                "HTTP/1.1 200 OK\r\nUSN: uuid:fake-sonos-1\r\nLOCATION: http://{http_addr}/xml/device_description.xml\r\nST: urn:schemas-upnp-org:device:ZonePlayer:1\r\n\r\n"
            );
            let _ = responder_socket.send_to(reply.as_bytes(), src).await;
        }
    });

    // search_once binds its own ephemeral socket and unicasts M-SEARCH to the
    // fake responder's address directly (loopback stand-in for the multicast group).
    let devices = search_once_to(responder_addr, Duration::from_secs(2))
        .await
        .unwrap();

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].usn, "uuid:fake-sonos-1");

    let client = reqwest::Client::new();
    let name = fetch_friendly_name(&client, &devices[0].location).await;
    assert_eq!(name.as_deref(), Some("Living Room"));
}

// Test-only helper: same as `search_once` but targets an arbitrary unicast
// address instead of the SSDP multicast group, so the test doesn't depend on
// multicast routing being available in CI.
async fn search_once_to(
    target: std::net::SocketAddr,
    timeout: Duration,
) -> std::io::Result<Vec<SonosDevice>> {
    on_air_core::sender::sonos::discovery::search_once_impl(target, timeout).await
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p on-air-core --test sonos_discovery`
Expected: FAIL to compile — `on_air_core::sender::sonos` does not exist yet.

- [ ] **Step 4: Implement SSDP discovery**

```rust
// packages/core/src/sender/sonos/discovery.rs
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout as tokio_timeout;

pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";
pub const SSDP_SEARCH_TARGET: &str = "urn:schemas-upnp-org:device:ZonePlayer:1";
pub const DEVICE_TTL: Duration = Duration::from_secs(120);
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct SonosDevice {
    pub usn: String,
    pub location: String,
    pub ip: IpAddr,
    pub friendly_name: String,
}

impl SonosDevice {
    fn control_base_url(&self) -> String {
        let without_scheme = self.location.trim_start_matches("http://");
        let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
        format!("http://{host_port}")
    }

    pub fn av_transport_control_url(&self) -> String {
        format!("{}/MediaRenderer/AVTransport/Control", self.control_base_url())
    }

    pub fn rendering_control_url(&self) -> String {
        format!(
            "{}/MediaRenderer/RenderingControl/Control",
            self.control_base_url()
        )
    }
}

#[derive(Default)]
pub struct DeviceRegistry {
    devices: HashMap<String, (SonosDevice, Instant)>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, device: SonosDevice, seen_at: Instant) {
        self.devices.insert(device.usn.clone(), (device, seen_at));
    }

    pub fn expire_stale(&mut self, now: Instant, ttl: Duration) {
        self.devices
            .retain(|_, (_, seen_at)| now.duration_since(*seen_at) < ttl);
    }

    pub fn list(&self) -> Vec<SonosDevice> {
        self.devices.values().map(|(d, _)| d.clone()).collect()
    }
}

fn build_msearch() -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {SSDP_MULTICAST_ADDR}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {SSDP_SEARCH_TARGET}\r\n\r\n"
    )
}

fn parse_ssdp_response(data: &[u8], source_ip: IpAddr) -> Option<SonosDevice> {
    let text = std::str::from_utf8(data).ok()?;
    let mut usn = None;
    let mut location = None;
    for line in text.split("\r\n") {
        let mut parts = line.splitn(2, ':');
        let key = parts.next()?.trim().to_ascii_uppercase();
        let Some(value) = parts.next() else { continue };
        let value = value.trim().to_string();
        match key.as_str() {
            "USN" => usn = Some(value),
            "LOCATION" => location = Some(value),
            _ => {}
        }
    }
    Some(SonosDevice {
        usn: usn?,
        location: location.clone()?,
        ip: source_ip,
        friendly_name: source_ip.to_string(),
    })
}

async fn search_to(target: SocketAddr, duration: Duration) -> std::io::Result<Vec<SonosDevice>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let msearch = build_msearch();
    socket.send_to(msearch.as_bytes(), target).await?;

    let mut found = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio_timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                if let Some(device) = parse_ssdp_response(&buf[..len], src.ip()) {
                    found.push(device);
                }
            }
            _ => break,
        }
    }
    Ok(found)
}

/// Real discovery: broadcasts M-SEARCH to the SSDP multicast group.
pub async fn search_once(duration: Duration) -> std::io::Result<Vec<SonosDevice>> {
    let target: SocketAddr = SSDP_MULTICAST_ADDR.parse().expect("valid multicast addr");
    search_to(target, duration).await
}

/// Test seam: same protocol, but unicast to an arbitrary address instead of
/// the multicast group, so tests don't depend on multicast routing in CI.
#[doc(hidden)]
pub async fn search_once_impl(
    target: SocketAddr,
    duration: Duration,
) -> std::io::Result<Vec<SonosDevice>> {
    search_to(target, duration).await
}

fn extract_xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

pub async fn fetch_friendly_name(client: &reqwest::Client, location: &str) -> Option<String> {
    let body = client.get(location).send().await.ok()?.text().await.ok()?;
    extract_xml_tag_text(&body, "roomName").or_else(|| extract_xml_tag_text(&body, "friendlyName"))
}
```

```rust
// packages/core/src/sender/sonos/mod.rs
pub mod discovery;
```

- [ ] **Step 5: Wire `sonos` into `sender`**

```rust
// packages/core/src/sender/mod.rs — add near the top
pub mod sonos;
```

- [ ] **Step 6: Add `outputs` field to `CoreState` and a periodic discovery task**

```rust
// packages/core/src/state.rs
use crate::sender::sonos::discovery::DeviceRegistry;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
    pub outputs: Arc<Mutex<DeviceRegistry>>,
}
```

```rust
// in CoreState::new() — add to the struct literal
outputs: Arc::new(Mutex::new(DeviceRegistry::new())),
```

```rust
// packages/core/src/state.rs — new method on CoreState
impl CoreState {
    /// Spawns a background task that periodically SSDP-searches for Sonos
    /// devices and merges results into `self.outputs`, expiring stale entries.
    pub fn spawn_sonos_discovery(&self) -> tokio::task::JoinHandle<()> {
        use crate::sender::sonos::discovery::{search_once, DEVICE_TTL, DISCOVERY_INTERVAL};

        let outputs = self.outputs.clone();
        tokio::spawn(async move {
            let http = reqwest::Client::new();
            loop {
                if let Ok(found) = search_once(Duration::from_secs(2)).await {
                    let now = Instant::now();
                    let mut registry = outputs.lock().await;
                    for mut device in found {
                        if let Some(name) =
                            crate::sender::sonos::discovery::fetch_friendly_name(&http, &device.location)
                                .await
                        {
                            device.friendly_name = name;
                        }
                        registry.upsert(device, now);
                    }
                    registry.expire_stale(now, DEVICE_TTL);
                }
                tokio::time::sleep(DISCOVERY_INTERVAL).await;
            }
        })
    }
}
```

- [ ] **Step 7: Implement `GET /api/outputs`**

```rust
// packages/core/src/api/outputs.rs
use crate::state::CoreState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    pub transport: &'static str,
}

#[derive(Serialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

pub async fn list_outputs(State(state): State<CoreState>) -> Json<OutputsResponse> {
    let devices = state.outputs.lock().await.list();
    Json(OutputsResponse {
        outputs: devices
            .into_iter()
            .map(|d| OutputInfo {
                id: d.usn,
                name: d.friendly_name,
                transport: "sonos",
            })
            .collect(),
    })
}
```

- [ ] **Step 8: Wire `outputs` into `api` and register the route**

```rust
// packages/core/src/api/mod.rs
pub mod inputs;
pub mod outputs;
pub mod stream;
```

```rust
// packages/core/src/lib.rs
pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .with_state(state)
}
```

Note: `serve`/`serve_on` do not call `spawn_sonos_discovery` yet — that's wired in Task 12's real-hardware validation setup notes, since starting a background UDP multicast task on every test-constructed `CoreState::new()` would make unit tests noisy. It's exposed as an explicit opt-in call.

- [ ] **Step 9: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test sonos_discovery`
Expected: PASS (3/3)

Run: `cargo test -p on-air-core`
Expected: all tests still green.

- [ ] **Step 10: Commit**

```bash
git add packages/core/src/sender/sonos packages/core/src/sender/mod.rs \
        packages/core/src/api/outputs.rs packages/core/src/api/mod.rs \
        packages/core/src/state.rs packages/core/src/lib.rs \
        packages/core/tests/sonos_discovery.rs
git commit -m "feat(core): add Sonos SSDP discovery with TTL expiry, /api/outputs"
```

---

### Task 8: Sonos SOAP control client

**Files:**
- Create: `packages/core/src/sender/sonos/soap.rs`
- Modify: `packages/core/src/sender/sonos/mod.rs` (`pub mod soap;`)
- Test: `packages/core/tests/sonos_soap.rs`

**Interfaces:**
- Consumes: `reqwest` as a regular dependency (moved from `dev-dependencies` in Task 7).
- Produces: `sender::sonos::soap::{SonosControlClient, SoapError}`. `SonosControlClient::new(http: reqwest::Client) -> Self`, `set_av_transport_uri(&self, control_url: &str, stream_uri: &str) -> Result<(), SoapError>`, `play(&self, control_url: &str) -> Result<(), SoapError>`, `stop(&self, control_url: &str) -> Result<(), SoapError>`, `set_volume(&self, rendering_control_url: &str, volume: u8) -> Result<(), SoapError>`.

- [ ] **Step 1: Write the failing tests**

```rust
// packages/core/tests/sonos_soap.rs
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use on_air_core::sender::sonos::soap::SonosControlClient;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(String, String)>>>); // (soapaction, body)

async fn capture_handler(
    State(state): State<Captured>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    let action = headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    state.0.lock().unwrap().push((action, body));
    StatusCode::OK
}

async fn start_fake_upnp_server() -> (std::net::SocketAddr, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/Control", post(capture_handler))
        .with_state(captured.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, captured)
}

#[tokio::test]
async fn set_av_transport_uri_sends_expected_action_and_body() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let control_url = format!("http://{addr}/Control");

    client
        .set_av_transport_uri(&control_url, "http://192.168.1.10:47990/stream/audio.wav")
        .await
        .unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.contains("SetAVTransportURI"));
    assert!(calls[0].1.contains("<CurrentURI>http://192.168.1.10:47990/stream/audio.wav</CurrentURI>"));
}

#[tokio::test]
async fn play_and_stop_send_expected_actions() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let control_url = format!("http://{addr}/Control");

    client.play(&control_url).await.unwrap();
    client.stop(&control_url).await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].0.contains("#Play"));
    assert!(calls[1].0.contains("#Stop"));
}

#[tokio::test]
async fn set_volume_sends_desired_volume() {
    let (addr, captured) = start_fake_upnp_server().await;
    let client = SonosControlClient::new(reqwest::Client::new());
    let rendering_url = format!("http://{addr}/Control");

    client.set_volume(&rendering_url, 42).await.unwrap();

    let calls = captured.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.contains("SetVolume"));
    assert!(calls[0].1.contains("<DesiredVolume>42</DesiredVolume>"));
}

#[tokio::test]
async fn non_success_status_is_an_error() {
    let app = Router::new().route(
        "/Control",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "soap fault") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = SonosControlClient::new(reqwest::Client::new());
    let result = client.play(&format!("http://{addr}/Control")).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("500"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p on-air-core --test sonos_soap`
Expected: FAIL to compile — `on_air_core::sender::sonos::soap` does not exist yet.

- [ ] **Step 3: Implement the SOAP client**

```rust
// packages/core/src/sender/sonos/soap.rs
use reqwest::Client;

#[derive(Debug)]
pub struct SoapError {
    pub action: &'static str,
    pub message: String,
}

impl std::fmt::Display for SoapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SOAP {} failed: {}", self.action, self.message)
    }
}

impl std::error::Error for SoapError {}

pub struct SonosControlClient {
    http: Client,
}

impl SonosControlClient {
    pub fn new(http: Client) -> Self {
        SonosControlClient { http }
    }

    pub async fn set_av_transport_uri(
        &self,
        control_url: &str,
        stream_uri: &str,
    ) -> Result<(), SoapError> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <CurrentURI>{stream_uri}</CurrentURI>
      <CurrentURIMetaData></CurrentURIMetaData>
    </u:SetAVTransportURI>
  </s:Body>
</s:Envelope>"#
        );
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI",
            body,
            "SetAVTransportURI",
        )
        .await
    }

    pub async fn play(&self, control_url: &str) -> Result<(), SoapError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>"#
            .to_string();
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#Play",
            body,
            "Play",
        )
        .await
    }

    pub async fn stop(&self, control_url: &str) -> Result<(), SoapError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Stop xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
    </u:Stop>
  </s:Body>
</s:Envelope>"#
            .to_string();
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#Stop",
            body,
            "Stop",
        )
        .await
    }

    pub async fn set_volume(&self, rendering_control_url: &str, volume: u8) -> Result<(), SoapError> {
        let volume = volume.min(100);
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetVolume xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
      <InstanceID>0</InstanceID>
      <Channel>Master</Channel>
      <DesiredVolume>{volume}</DesiredVolume>
    </u:SetVolume>
  </s:Body>
</s:Envelope>"#
        );
        self.send_action(
            rendering_control_url,
            "urn:schemas-upnp-org:service:RenderingControl:1#SetVolume",
            body,
            "SetVolume",
        )
        .await
    }

    async fn send_action(
        &self,
        control_url: &str,
        soap_action: &str,
        body: String,
        action_name: &'static str,
    ) -> Result<(), SoapError> {
        let response = self
            .http
            .post(control_url)
            .header("Content-Type", r#"text/xml; charset="utf-8""#)
            .header("SOAPACTION", format!("\"{soap_action}\""))
            .body(body)
            .send()
            .await
            .map_err(|e| SoapError {
                action: action_name,
                message: e.to_string(),
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(SoapError {
                action: action_name,
                message: format!("HTTP {status}: {text}"),
            })
        }
    }
}
```

- [ ] **Step 4: Wire `soap` into `sonos`**

```rust
// packages/core/src/sender/sonos/mod.rs
pub mod discovery;
pub mod soap;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test sonos_soap`
Expected: PASS (4/4)

- [ ] **Step 6: Commit**

```bash
git add packages/core/src/sender/sonos packages/core/Cargo.toml packages/core/Cargo.lock \
        packages/core/tests/sonos_soap.rs
git commit -m "feat(core): add hand-rolled Sonos UPnP/SOAP control client"
```

---

### Task 9: `SonosSender` + activate/volume API

**Files:**
- Create: `packages/core/src/sender/sonos/net.rs`
- Modify: `packages/core/src/sender/sonos/mod.rs` (add `SonosSender`, `pub mod net;`)
- Modify: `packages/core/src/api/outputs.rs` (add `activate_output`, `set_output_volume` handlers)
- Modify: `packages/core/src/lib.rs` (register `POST /api/outputs/active`, `POST /api/outputs/active/volume`)
- Test: `packages/core/tests/sonos_sender.rs`
- Test: `packages/core/tests/api_outputs.rs`

**Interfaces:**
- Consumes: `sender::AudioSender` (Task 3), `sender::sonos::discovery::SonosDevice` (Task 7), `sender::sonos::soap::SonosControlClient` (Task 8), `state::CoreState.activate_sender`/`deactivate_sender` (Task 3).
- Produces: `sender::sonos::SonosSender::new(device: SonosDevice, http: reqwest::Client, stream_url: String) -> SonosSender` (implements `AudioSender`). `sender::sonos::net::local_lan_ip() -> std::io::Result<std::net::IpAddr>`. Routes: `POST /api/outputs/active`, `POST /api/outputs/active/volume`.

- [ ] **Step 1: Write the failing tests**

```rust
// packages/core/tests/sonos_sender.rs
use axum::extract::State;
use axum::http::HeaderMap;
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

async fn capture_handler(State(state): State<Captured>, headers: HeaderMap, _body: String) -> &'static str {
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
        .route("/MediaRenderer/RenderingControl/Control", post(capture_handler))
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
    let device = SonosDevice {
        usn: "uuid:fake".into(),
        location: format!("http://{addr}/xml/device_description.xml"),
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        friendly_name: "Fake Speaker".into(),
    };

    let mut sender = SonosSender::new(
        device,
        reqwest::Client::new(),
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
```

```rust
// packages/core/tests/api_outputs.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn activate_unknown_output_returns_404() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"transport":"sonos","device_id":"does-not-exist"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn volume_with_no_active_output_returns_409() {
    let app = on_air_core::build_router(CoreState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/outputs/active/volume")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"volume":50}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p on-air-core --test sonos_sender --test api_outputs`
Expected: FAIL to compile — `SonosSender` and the new routes don't exist yet.

- [ ] **Step 3: Implement `local_lan_ip`**

```rust
// packages/core/src/sender/sonos/net.rs
use std::net::{IpAddr, UdpSocket};

/// Determines this machine's LAN-facing IP by asking the OS routing table
/// which local address it would use to reach an external address — no
/// packet is actually sent (UDP `connect` just fixes the default peer and
/// lets the kernel pick a source address).
pub fn local_lan_ip() -> std::io::Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip())
}
```

- [ ] **Step 4: Implement `SonosSender`**

```rust
// packages/core/src/sender/sonos/mod.rs
pub mod discovery;
pub mod net;
pub mod soap;

use crate::sender::{AudioSender, SenderError};
use discovery::SonosDevice;
use soap::SonosControlClient;

pub struct SonosSender {
    device: SonosDevice,
    client: SonosControlClient,
    stream_url: String,
}

impl SonosSender {
    pub fn new(device: SonosDevice, http: reqwest::Client, stream_url: String) -> Self {
        SonosSender {
            device,
            client: SonosControlClient::new(http),
            stream_url,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for SonosSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.client
            .set_av_transport_uri(&self.device.av_transport_control_url(), &self.stream_url)
            .await
            .map_err(|e| SenderError(e.to_string()))?;
        self.client
            .play(&self.device.av_transport_control_url())
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.client
            .stop(&self.device.av_transport_control_url())
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.client
            .set_volume(&self.device.rendering_control_url(), volume)
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.device.friendly_name
    }
}
```

- [ ] **Step 5: Add activate/volume handlers**

```rust
// packages/core/src/api/outputs.rs — add below the existing list_outputs code
use crate::sender::sonos::{net::local_lan_ip, SonosSender};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

pub async fn activate_output(
    State(state): State<CoreState>,
    Json(req): Json<ActivateOutputRequest>,
) -> Response {
    if req.transport != "sonos" {
        return (StatusCode::BAD_REQUEST, "only 'sonos' is supported in M1").into_response();
    }

    let device = {
        let registry = state.outputs.lock().await;
        registry.list().into_iter().find(|d| d.usn == req.device_id)
    };
    let Some(device) = device else {
        return (StatusCode::NOT_FOUND, "output device not found").into_response();
    };

    let lan_ip = match local_lan_ip() {
        Ok(ip) => ip,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let stream_url = format!(
        "http://{lan_ip}:{}/stream/audio.wav",
        crate::DEFAULT_PORT
    );

    let sender = SonosSender::new(device, reqwest::Client::new(), stream_url);
    match state.activate_sender(Box::new(sender)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

pub async fn set_output_volume(
    State(state): State<CoreState>,
    Json(req): Json<SetVolumeRequest>,
) -> Response {
    let mut guard = state.active_sender.lock().await;
    match guard.as_mut() {
        Some(sender) => match sender.set_volume(req.volume).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        None => (StatusCode::CONFLICT, "no active output").into_response(),
    }
}
```

- [ ] **Step 6: Register the new routes**

```rust
// packages/core/src/lib.rs
pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route("/api/outputs/active", post(api::outputs::activate_output))
        .route(
            "/api/outputs/active/volume",
            post(api::outputs::set_output_volume),
        )
        .with_state(state)
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test sonos_sender --test api_outputs`
Expected: PASS (3/3)

Run: `cargo test -p on-air-core`
Expected: all tests still green.

- [ ] **Step 8: Commit**

```bash
git add packages/core/src/sender/sonos packages/core/src/api/outputs.rs packages/core/src/lib.rs \
        packages/core/tests/sonos_sender.rs packages/core/tests/api_outputs.rs
git commit -m "feat(core): add SonosSender and /api/outputs/active[,/volume] endpoints"
```

---

### Task 10: `/api/eq` and `/api/sample-rate`

**Files:**
- Create: `packages/core/src/api/eq.rs`
- Create: `packages/core/src/api/sample_rate.rs`
- Modify: `packages/core/src/api/mod.rs` (`pub mod eq; pub mod sample_rate;`)
- Modify: `packages/core/src/lib.rs` (register routes)
- Test: `packages/core/tests/api_settings.rs`

**Interfaces:**
- Consumes: `state::CoreState.eq_gains_db`, `state::CoreState.target_sample_rate_hz` (Task 3).
- Produces: Routes `GET /api/eq`, `PUT /api/eq`, `GET /api/sample-rate`, `PUT /api/sample-rate`.

- [ ] **Step 1: Write the failing tests**

```rust
// packages/core/tests/api_settings.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use on_air_core::state::CoreState;
use tower::ServiceExt;

#[tokio::test]
async fn eq_defaults_to_zero_and_can_be_updated() {
    let app = on_air_core::build_router(CoreState::new());

    let get_response = app
        .clone()
        .oneshot(Request::builder().uri("/api/eq").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["gains_db"], serde_json::json!([0.0, 0.0, 0.0, 0.0, 0.0]));

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/eq")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"gains_db":[3.0,0.0,0.0,0.0,-3.0]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response_2 = app
        .oneshot(Request::builder().uri("/api/eq").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(get_response_2.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["gains_db"], serde_json::json!([3.0, 0.0, 0.0, 0.0, -3.0]));
}

#[tokio::test]
async fn sample_rate_defaults_to_44100_and_can_be_updated() {
    let app = on_air_core::build_router(CoreState::new());

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sample-rate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["sample_rate_hz"], 44100);

    let put_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/sample-rate")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sample_rate_hz":48000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p on-air-core --test api_settings`
Expected: FAIL — routes don't exist yet (404s where 200/204 expected).

- [ ] **Step 3: Implement the handlers**

```rust
// packages/core/src/api/eq.rs
use crate::dsp::eq::EQ_GAIN_RANGE_DB;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct EqResponse {
    pub gains_db: [f32; 5],
}

pub async fn get_eq(State(state): State<CoreState>) -> Json<EqResponse> {
    Json(EqResponse {
        gains_db: *state.eq_gains_db.lock().unwrap(),
    })
}

#[derive(Deserialize)]
pub struct SetEqRequest {
    pub gains_db: [f32; 5],
}

pub async fn set_eq(State(state): State<CoreState>, Json(req): Json<SetEqRequest>) -> StatusCode {
    let clamped = req
        .gains_db
        .map(|g| g.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1));
    *state.eq_gains_db.lock().unwrap() = clamped;
    StatusCode::NO_CONTENT
}
```

```rust
// packages/core/src/api/sample_rate.rs
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SampleRateResponse {
    pub sample_rate_hz: u32,
}

pub async fn get_sample_rate(State(state): State<CoreState>) -> Json<SampleRateResponse> {
    Json(SampleRateResponse {
        sample_rate_hz: *state.target_sample_rate_hz.lock().unwrap(),
    })
}

#[derive(Deserialize)]
pub struct SetSampleRateRequest {
    pub sample_rate_hz: u32,
}

pub async fn set_sample_rate(
    State(state): State<CoreState>,
    Json(req): Json<SetSampleRateRequest>,
) -> StatusCode {
    *state.target_sample_rate_hz.lock().unwrap() = req.sample_rate_hz;
    StatusCode::NO_CONTENT
}
```

- [ ] **Step 4: Wire modules in and register routes**

```rust
// packages/core/src/api/mod.rs
pub mod eq;
pub mod inputs;
pub mod outputs;
pub mod sample_rate;
pub mod stream;
```

```rust
// packages/core/src/lib.rs
use axum::routing::put;

pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route("/api/outputs/active", post(api::outputs::activate_output))
        .route(
            "/api/outputs/active/volume",
            post(api::outputs::set_output_volume),
        )
        .route(
            "/api/eq",
            get(api::eq::get_eq).put(api::eq::set_eq),
        )
        .route(
            "/api/sample-rate",
            get(api::sample_rate::get_sample_rate).put(api::sample_rate::set_sample_rate),
        )
        .with_state(state)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p on-air-core --test api_settings`
Expected: PASS (2/2)

- [ ] **Step 6: Commit**

```bash
git add packages/core/src/api/eq.rs packages/core/src/api/sample_rate.rs \
        packages/core/src/api/mod.rs packages/core/src/lib.rs packages/core/tests/api_settings.rs
git commit -m "feat(core): add /api/eq and /api/sample-rate endpoints"
```

---

### Task 11: WebSocket `/api/ws` events

**Files:**
- Create: `packages/core/src/api/ws.rs`
- Modify: `packages/core/src/api/mod.rs` (`pub mod ws;`)
- Modify: `packages/core/src/state.rs` (add `ws_tx` field, publish `OutputStateChanged` from `activate_sender`/`deactivate_sender`, publish `DeviceJoined`/`DeviceLeft` from `spawn_sonos_discovery`)
- Modify: `packages/core/src/pipeline/mod.rs` (`spawn_processing_task` also publishes `LevelMeter`)
- Modify: `packages/core/src/api/inputs.rs` (pass `state.ws_tx.clone()` into `spawn_processing_task`)
- Modify: `packages/core/src/lib.rs` (`Cargo.toml` add `ws` feature to axum, register `GET /api/ws`)
- Modify: `packages/core/Cargo.toml` (axum `ws` feature, dev-dependency `tokio-tungstenite`)
- Test: `packages/core/tests/ws_events.rs`

**Interfaces:**
- Produces: `api::ws::WsEvent` (`#[derive(Clone, Serialize)]`, `#[serde(tag = "type")]`): `OutputStateChanged { transport: String, device_name: String, active: bool }`, `LevelMeter { rms: f32, peak: f32 }`, `DeviceJoined { transport: String, id: String, name: String }`, `DeviceLeft { transport: String, id: String }`. `CoreState.ws_tx: broadcast::Sender<WsEvent>`. Route: `GET /api/ws`.

- [ ] **Step 1: Add dependencies**

```toml
# packages/core/Cargo.toml
[dependencies]
axum = { version = "0.7", features = ["ws"] }
# ... rest unchanged ...

[dev-dependencies]
tokio-tungstenite = "0.30.0"
```

- [ ] **Step 2: Write the failing test**

```rust
// packages/core/tests/ws_events.rs
use futures_util::{SinkExt, StreamExt};
use on_air_core::api::ws::WsEvent;
use on_air_core::state::CoreState;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn ws_forwards_published_events_as_json() {
    let state = CoreState::new();
    let ws_tx = state.ws_tx.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = on_air_core::build_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (mut ws_stream, _) = connect_async(format!("ws://{addr}/api/ws")).await.unwrap();

    let event = WsEvent::LevelMeter { rms: 0.5, peak: 0.9 };
    let _ = ws_tx.send(event.clone());

    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next())
        .await
        .expect("received a message before timing out")
        .expect("stream not closed")
        .expect("message read ok");

    let Message::Text(text) = msg else {
        panic!("expected a text message, got {msg:?}");
    };
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "LevelMeter");
    assert_eq!(parsed["rms"], 0.5);

    let _ = ws_stream.close(None).await;
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p on-air-core --test ws_events`
Expected: FAIL to compile — `on_air_core::api::ws` does not exist yet.

- [ ] **Step 4: Implement the WebSocket handler**

```rust
// packages/core/src/api/ws.rs
use crate::state::CoreState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    OutputStateChanged {
        transport: String,
        device_name: String,
        active: bool,
    },
    LevelMeter {
        rms: f32,
        peak: f32,
    },
    DeviceJoined {
        transport: String,
        id: String,
        name: String,
    },
    DeviceLeft {
        transport: String,
        id: String,
    },
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<CoreState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: CoreState) {
    let mut rx = state.ws_tx.subscribe();
    while let Ok(event) = rx.recv().await {
        let Ok(text) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(text)).await.is_err() {
            break;
        }
    }
}
```

- [ ] **Step 5: Add `ws_tx` to `CoreState` and publish `OutputStateChanged`**

```rust
// packages/core/src/state.rs
use crate::api::ws::WsEvent;

#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
    pub outputs: Arc<Mutex<DeviceRegistry>>,
    pub ws_tx: broadcast::Sender<WsEvent>,
}
```

```rust
// in CoreState::new()
let (ws_tx, _) = broadcast::channel(64);
// ... add `ws_tx,` to the struct literal
```

```rust
// packages/core/src/state.rs — modify activate_sender/deactivate_sender
pub async fn activate_sender(&self, new_sender: Box<dyn AudioSender>) -> Result<(), SenderError> {
    let mut guard = self.active_sender.lock().await;
    if let Some(mut current) = guard.take() {
        current.stop().await?;
        let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
            transport: "sonos".to_string(),
            device_name: current.name().to_string(),
            active: false,
        });
    }
    let mut new_sender = new_sender;
    new_sender.start().await?;
    let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
        transport: "sonos".to_string(),
        device_name: new_sender.name().to_string(),
        active: true,
    });
    *guard = Some(new_sender);
    Ok(())
}

pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
    let mut guard = self.active_sender.lock().await;
    if let Some(mut current) = guard.take() {
        current.stop().await?;
        let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
            transport: "sonos".to_string(),
            device_name: current.name().to_string(),
            active: false,
        });
    }
    Ok(())
}
```

- [ ] **Step 6: Publish `DeviceJoined`/`DeviceLeft` from discovery**

```rust
// packages/core/src/state.rs — replace spawn_sonos_discovery's body
pub fn spawn_sonos_discovery(&self) -> tokio::task::JoinHandle<()> {
    use crate::sender::sonos::discovery::{search_once, DEVICE_TTL, DISCOVERY_INTERVAL};

    let outputs = self.outputs.clone();
    let ws_tx = self.ws_tx.clone();
    tokio::spawn(async move {
        let http = reqwest::Client::new();
        loop {
            let before: std::collections::HashSet<String> = {
                let registry = outputs.lock().await;
                registry.list().into_iter().map(|d| d.usn).collect()
            };

            if let Ok(found) = search_once(Duration::from_secs(2)).await {
                let now = Instant::now();
                let mut registry = outputs.lock().await;
                for mut device in found {
                    if let Some(name) =
                        crate::sender::sonos::discovery::fetch_friendly_name(&http, &device.location)
                            .await
                    {
                        device.friendly_name = name;
                    }
                    if !before.contains(&device.usn) {
                        let _ = ws_tx.send(WsEvent::DeviceJoined {
                            transport: "sonos".to_string(),
                            id: device.usn.clone(),
                            name: device.friendly_name.clone(),
                        });
                    }
                    registry.upsert(device, now);
                }
                registry.expire_stale(now, DEVICE_TTL);

                let after: std::collections::HashSet<String> =
                    registry.list().into_iter().map(|d| d.usn).collect();
                for left in before.difference(&after) {
                    let _ = ws_tx.send(WsEvent::DeviceLeft {
                        transport: "sonos".to_string(),
                        id: left.clone(),
                    });
                }
            }
            tokio::time::sleep(DISCOVERY_INTERVAL).await;
        }
    })
}
```

- [ ] **Step 7: Publish `LevelMeter` from the processing thread**

```rust
// packages/core/src/pipeline/mod.rs — extend spawn_processing_task's signature and body
use crate::api::ws::WsEvent;

pub fn spawn_processing_task(
    mut consumer: HeapCons<f32>,
    input_rate_hz: u32,
    output_rate_hz: u32,
    eq_gains_db: Arc<Mutex<[f32; 5]>>,
    audio_tx: broadcast::Sender<Bytes>,
    ws_tx: broadcast::Sender<WsEvent>,
) -> ProcessingTaskHandle {
    // ... unchanged setup ...
    let join_handle = std::thread::spawn(move || {
        let mut eq = GraphicEq::new(input_rate_hz as f32);
        let mut resampler = MonoResampler::new(input_rate_hz, output_rate_hz);
        let chunk_frames = resampler.input_frames_next();
        let mut chunk = vec![0.0f32; chunk_frames];

        while !stop_flag_thread.load(Ordering::Relaxed) {
            let mut filled = 0;
            while filled < chunk_frames {
                if stop_flag_thread.load(Ordering::Relaxed) {
                    return;
                }
                filled += consumer.pop_slice(&mut chunk[filled..]);
                if filled < chunk_frames {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }

            eq.set_gains_db(*eq_gains_db.lock().unwrap());
            eq.process(&mut chunk);

            let peak = chunk.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            let _ = ws_tx.send(WsEvent::LevelMeter { rms, peak });

            let resampled = resampler.process(&chunk);
            let pcm_bytes = f32_to_le_i16_bytes(&resampled);
            let _ = audio_tx.send(pcm_bytes);
        }
    });
    // ... unchanged return ...
}
```

Update the Task 4 test (`pipeline_processing.rs`) and the Task 6 caller (`api/inputs.rs`'s `activate_input`) to pass a `ws_tx` argument:

```rust
// packages/core/tests/pipeline_processing.rs — add alongside the existing broadcast::channel setup
let (ws_tx, _) = broadcast::channel::<on_air_core::api::ws::WsEvent>(8);
// ... and update the call:
let handle = spawn_processing_task(consumer, 44100, 44100, eq_gains_db, audio_tx, ws_tx);
```

```rust
// packages/core/src/api/inputs.rs — update the spawn_processing_task call
let processing = pipeline::spawn_processing_task(
    consumer,
    input_rate,
    target_rate,
    state.eq_gains_db.clone(),
    state.audio_tx.clone(),
    state.ws_tx.clone(),
);
```

- [ ] **Step 8: Wire `ws` into `api` and register the route**

```rust
// packages/core/src/api/mod.rs
pub mod eq;
pub mod inputs;
pub mod outputs;
pub mod sample_rate;
pub mod stream;
pub mod ws;
```

```rust
// packages/core/src/lib.rs
pub fn build_router(state: CoreState) -> Router {
    Router::new()
        .route("/api/status", get(status_handler))
        .route("/stream/audio.wav", get(api::stream::stream_audio))
        .route("/api/inputs", get(api::inputs::list_inputs))
        .route("/api/inputs/active", post(api::inputs::activate_input))
        .route("/api/outputs", get(api::outputs::list_outputs))
        .route("/api/outputs/active", post(api::outputs::activate_output))
        .route(
            "/api/outputs/active/volume",
            post(api::outputs::set_output_volume),
        )
        .route("/api/eq", get(api::eq::get_eq).put(api::eq::set_eq))
        .route(
            "/api/sample-rate",
            get(api::sample_rate::get_sample_rate).put(api::sample_rate::set_sample_rate),
        )
        .route("/api/ws", get(api::ws::ws_handler))
        .with_state(state)
}
```

- [ ] **Step 9: Run all tests to verify everything passes**

Run: `cargo test -p on-air-core`
Expected: PASS — every test file green, including the updated `pipeline_processing` call site.

- [ ] **Step 10: Commit**

```bash
git add packages/core/src packages/core/tests packages/core/Cargo.toml packages/core/Cargo.lock
git commit -m "feat(core): add /api/ws with output-state, level-meter, device join/leave events"
```

---

### Task 12: Manual real-hardware validation

**Not a coding task.** This is done by you directly with a real Sonos speaker on the same LAN as this machine — a subagent implementer has no access to that hardware, so this task is executed by the controller together with the user, the same pattern M0 used for its mobile↔desktop LAN check.

- [ ] **Step 1: Start `core` bound to all interfaces with Sonos discovery running**

Add a small temporary `examples/serve_m1.rs` (same throwaway pattern used for M0's live-verification), or run via a quick `cargo run` snippet that calls `on_air_core::serve_on` after also calling `state.spawn_sonos_discovery()` — since `serve`/`serve_on` construct a default `CoreState` internally without starting discovery, the validation step needs a small wrapper that constructs `CoreState::new()`, calls `.spawn_sonos_discovery()`, then serves `build_router(state)` directly via `axum::serve` on `0.0.0.0:47990`.

- [ ] **Step 2: Confirm discovery**

`curl http://127.0.0.1:47990/api/outputs` — wait up to ~30s (one discovery interval) and confirm the real Sonos speaker appears with its correct room name.

- [ ] **Step 3: Confirm capture**

`curl http://127.0.0.1:47990/api/inputs` — confirm the PipeWire monitor source appears in the list. `curl -X POST http://127.0.0.1:47990/api/inputs/active -d '{"name":"<device name>"}' -H 'content-type: application/json'` — confirm no error.

- [ ] **Step 4: Confirm playback**

`curl -X POST http://127.0.0.1:47990/api/outputs/active -d '{"transport":"sonos","device_id":"<usn from step 2>"}' -H 'content-type: application/json'` — confirm audio plays on the real speaker and sounds correct (no glitches, correct pitch — a wrong sample-rate assumption would be audible as sped-up/slowed-down audio).

- [ ] **Step 5: Confirm volume**

`curl -X POST http://127.0.0.1:47990/api/outputs/active/volume -d '{"volume":20}' -H 'content-type: application/json'` — confirm the physical speaker's volume changes to match (check against the Sonos app if available).

- [ ] **Step 6: Confirm EQ is audible**

`curl -X PUT http://127.0.0.1:47990/api/eq -d '{"gains_db":[12,0,0,0,-12]}' -H 'content-type: application/json'` — confirm a perceptible bass-boost/treble-cut change in the live audio.

- [ ] **Step 7: Confirm stop/switch behaves cleanly**

`curl -X POST http://127.0.0.1:47990/api/outputs/active -d '{"transport":"sonos","device_id":"<same usn>"}' ...` again — confirm no duplicate/overlapping audio (exclusivity correctly stopped the prior sender first per Task 3's design).

- [ ] **Step 8: Record results and clean up**

Note any issues found (e.g., `CurrentURIMetaData` needing real DIDL-Lite metadata if the speaker's transport misbehaves — flagged as a known simplification risk in the design spec). Remove the temporary example file. If everything in Steps 2–7 passed, M1 is done.
