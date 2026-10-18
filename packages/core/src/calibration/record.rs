//! The microphone side of the loopback: a stamped recording of what the
//! mic hears while the probe plays.
//!
//! The recorder reuses the product's own capture backends
//! ([`crate::pipeline::capture`]): `parec` against a Pulse/PipeWire source
//! on Linux, cpal elsewhere. What it adds is **timestamps**: every drained
//! block is kept with the instant its read returned (or its callback ran),
//! and [`Recording::index_of`] turns those arrivals into a sample-index ↔
//! wall-clock mapping by least squares over the whole run. Per-block
//! arrival jitter therefore averages out instead of entering the delay
//! figure; the mapping's remaining error is the fit residual, not one
//! block's scheduling noise.
//!
//! Two things are deliberately NOT compensated, because they cannot be
//! measured from inside the machine:
//!
//! - the capture path's delivery latency (samples reach the process up to
//!   one capture buffer after they were recorded). [`RECORDER_BUFFER_MS`]
//!   bounds it tightly and the module docs in [`crate::calibration`] state
//!   the consequence;
//! - microphone electronics/OS processing delay. Constant per device.
//!
//! Both are constant across transports measured in one session, so
//! inter-transport alignment — what sync groups use — is unaffected.
//!
//! A capture input that is a monitor/loopback source is REFUSED
//! ([`is_forbidden_capture_input`]): measuring a transport against its own
//! digital monitor would report a near-zero "latency" that is a lie.

use crate::pipeline::capture;
use crate::pipeline::sources;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

/// The recorder's capture buffer request, in milliseconds. Small on
/// purpose: it bounds the capture path's delivery latency — one of the two
/// constant terms the loopback figure includes — tightly.
pub const RECORDER_BUFFER_MS: u32 = 20;

/// Preferred capture rate for the recorder. What the device actually
/// delivers is reported in the measurement.
const PREFERRED_RATE_HZ: u32 = 48_000;

/// A finished recording: mono f32 samples in arrival order, each block
/// stamped with when it arrived.
pub struct Recording {
    pub rate_hz: u32,
    blocks: Vec<(Instant, Vec<f32>)>,
    total: usize,
}

impl Recording {
    fn new(rate_hz: u32) -> Self {
        Recording {
            rate_hz,
            blocks: Vec::new(),
            total: 0,
        }
    }

    pub fn push(&mut self, at: Instant, mono: Vec<f32>) {
        self.total += mono.len();
        self.blocks.push((at, mono));
    }

    /// Samples recorded so far.
    pub fn len_samples(&self) -> usize {
        self.total
    }

    /// Every sample, mono, in order.
    pub fn mono(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.total);
        for (_, block) in &self.blocks {
            out.extend_from_slice(block);
        }
        out
    }

    /// The sample index `at` corresponds to, by least-squares fit of
    /// (block end index, block arrival instant) over the recording. The
    /// fit absorbs per-block delivery jitter; only its offset (the capture
    /// delivery latency) is left in, as a constant.
    pub fn index_of(&self, at: Instant) -> Option<f64> {
        let (first_arrival, first_block) = self.blocks.first()?;
        if self.blocks.len() == 1 {
            // One block: the nominal rate is all there is to extrapolate.
            let end = first_block.len() as f64;
            return Some(
                end + at.duration_since(*first_arrival).as_secs_f64() * f64::from(self.rate_hz),
            );
        }
        // Least squares: arrival = alpha + beta * end_index.
        let mut count = 0.0f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;
        let mut end_index = 0.0f64;
        for (arrival, block) in &self.blocks {
            end_index += block.len() as f64;
            // Seconds relative to the first arrival: keeps the arithmetic
            // small and monotonic.
            let y = arrival.duration_since(*first_arrival).as_secs_f64();
            count += 1.0;
            sum_x += end_index;
            sum_y += y;
            sum_xx += end_index * end_index;
            sum_xy += end_index * y;
        }
        let denom = count * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-9 {
            return None;
        }
        let beta = (count * sum_xy - sum_x * sum_y) / denom;
        let alpha = (sum_y - beta * sum_x) / count;
        if beta <= 0.0 {
            return None;
        }
        // index(at) inverts arrival = alpha + beta * index, relative to the
        // first arrival.
        let y = at.duration_since(*first_arrival).as_secs_f64();
        Some((y - alpha) / beta)
    }
}

/// The two ways samples reach a [`Recording`]: the product's capture
/// backends, or (mock mode only) a synthesized acoustic world.
enum Handle {
    Pulse(capture::PulseMonitorCapture),
    Cpal(cpal::Stream),
    Mock,
}

/// A live microphone recording. [`Recorder::finish`] stops capture and
/// takes the recording; dropping without finishing (an error path or a
/// panic) stops capture too, so a `parec` and its reader thread can never
/// outlive the run.
pub struct Recorder {
    rate_hz: u32,
    recording: Arc<Mutex<Recording>>,
    handle: Option<Handle>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

impl Recorder {
    /// Start recording `capture_input` (the label `GET /api/inputs`
    /// lists). Runs `pactl`/`parec` or opens a cpal device: call via
    /// `spawn_blocking` from async code.
    pub fn start(capture_input: &str) -> Result<Recorder, String> {
        if is_forbidden_capture_input(capture_input) {
            return Err(format!(
                "{capture_input} is a monitor/loopback source; pick a real microphone — \
                 measuring against the transport's own digital monitor would report ~0"
            ));
        }
        let recording = Arc::new(Mutex::new(Recording::new(PREFERRED_RATE_HZ)));
        let sink_recording = recording.clone();

        let (pulse_running, pulse_source) = match sources::query() {
            Some(sources) => (true, sources.resolve(capture_input)),
            None => (false, capture::is_pulse_monitor_name(capture_input).then(|| capture_input.to_string())),
        };
        if let Some(source) = pulse_source {
            let (handle, rate_hz) = capture::start_pulse_monitor_stamped(
                &source,
                Some(PREFERRED_RATE_HZ),
                RECORDER_BUFFER_MS,
                move |samples, at| push_mono(&sink_recording, samples, at),
            )?;
            recording.lock().rate_hz = rate_hz;
            return Ok(Recorder {
                rate_hz,
                recording,
                handle: Some(Handle::Pulse(handle)),
            });
        }
        if pulse_running {
            // Same rule as the production capture: with a Pulse server
            // running, raw ALSA devices are off limits.
            return Err("input device not found".into());
        }
        let host = cpal::default_host();
        let device = capture::find_input_device(&host, capture_input)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "input device not found".to_string())?;
        let (stream, rate_hz) = capture::start_capture_stamped_at(
            &device,
            Some(PREFERRED_RATE_HZ),
            RECORDER_BUFFER_MS,
            move |samples, at| push_mono(&sink_recording, samples, at),
        )
        .map_err(|e| e.to_string())?;
        recording.lock().rate_hz = rate_hz;
        Ok(Recorder {
            rate_hz,
            recording,
            handle: Some(Handle::Cpal(stream)),
        })
    }

    /// Mock mode only: no device is opened. The run synthesizes the
    /// acoustic world itself and pushes it with [`Recorder::push_mock`].
    pub fn start_mock(rate_hz: u32) -> Recorder {
        Recorder {
            rate_hz,
            recording: Arc::new(Mutex::new(Recording::new(rate_hz))),
            handle: Some(Handle::Mock),
        }
    }

    /// Mock mode: append one synthesized block at a planned instant.
    pub fn push_mock(&self, at: Instant, mono: Vec<f32>) {
        self.recording.lock().push(at, mono);
    }

    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// Stop capture and take the recording.
    pub fn finish(mut self) -> Recording {
        self.stop_capture();
        let mut recording = self.recording.lock();
        std::mem::replace(&mut *recording, Recording::new(self.rate_hz))
    }

    /// Stop the capture backend. Called by [`Recorder::finish`] and by
    /// `Drop`, so it runs exactly once.
    fn stop_capture(&mut self) {
        match self.handle.take() {
            Some(Handle::Pulse(handle)) => handle.stop(),
            // Dropping a cpal stream joins its worker thread.
            Some(Handle::Cpal(stream)) => drop(stream),
            Some(Handle::Mock) | None => {}
        }
    }
}

/// Collapse one interleaved stereo block to mono and record it.
fn push_mono(recording: &Arc<Mutex<Recording>>, samples: &[f32], at: Instant) {
    let mut mono = Vec::with_capacity(samples.len() / 2 + 1);
    for frame in samples.as_chunks::<2>().0 {
        mono.push((frame[0] + frame[1]) * 0.5);
    }
    recording.lock().push(at, mono);
}

/// Whether this capture input is a digital monitor/loopback and must not
/// be used as a calibration microphone.
pub fn is_forbidden_capture_input(name: &str) -> bool {
    name == sources::SYSTEM_AUDIO
        || capture::is_pulse_monitor_name(name)
        || capture::is_loopback_device_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn monitor_and_loopback_inputs_are_refused() {
        for name in [
            "System audio",
            "alsa_output.pci.analog-stereo.monitor",
            "BlackHole 2ch",
            "Stereo Mix",
        ] {
            assert!(
                is_forbidden_capture_input(name),
                "{name} must be refused as a calibration mic"
            );
        }
        assert!(!is_forbidden_capture_input("Built-in Microphone"));
    }

    #[test]
    fn the_sample_index_fit_absorbs_arrival_jitter() {
        let mut recording = Recording::new(48_000);
        let origin = Instant::now();
        let block = 480usize; // 10 ms blocks at 48 kHz
        let mut index = 0usize;
        // Alternate early/late arrivals by ±3 ms: a per-block reading of
        // the timestamps would see a 6 ms swing, the fit must not.
        for i in 0..50 {
            let jitter = if i % 2 == 0 {
                Duration::from_millis(3)
            } else {
                Duration::from_millis(0)
            };
            let at = origin + Duration::from_secs_f64((index + block) as f64 / 48_000.0) + jitter;
            recording.push(at, vec![0.0; block]);
            index += block;
        }
        // Sample 24_000 (0.5 s in) maps back to 0.5 s after the origin to
        // within a fraction of a sample.
        let mapped = recording
            .index_of(origin + Duration::from_secs_f64(0.5))
            .unwrap();
        assert!(
            (mapped - 24_000.0).abs() < 120.0,
            "fit residual must average jitter away, got {mapped}"
        );
    }
}
