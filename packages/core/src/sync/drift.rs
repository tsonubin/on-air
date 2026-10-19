//! Drift measurement and steering.
//!
//! A sync group pins every member's playout to one presentation epoch and
//! per-output delay lines, but two independent clocks still drift apart over
//! time: the DAC in a speaker consumes samples at its own rate, tens of ppm
//! away from this machine's. [`DriftController`] measures one consumer's
//! actual consumption rate against the timeline and produces the correction
//! that a variable-ratio resampler ([`crate::dsp::resample::SteerableResampler`])
//! applies to stretch or squeeze that consumer's feed.
//!
//! The signal is honest because it is just what the consumer did: the feed
//! path reports the cumulative frame count the consumer has accepted and the
//! instant of the report. Over each measurement window the observed rate is
//! `frames / elapsed`; its difference from the nominal rate is the
//! consumer's clock drift. No latency figures are involved, and nothing is
//! measured that the consumer has not actually consumed.
//!
//! Cost: a handful of float ops per report, once per second or so.

use on_air_api::SyncState;
use std::time::{Duration, Instant};

/// Windows shorter than this are not rate measurements; reports accumulate
/// into a longer window instead.
const MIN_WINDOW: Duration = Duration::from_millis(500);
/// A consumer outside ±20% of nominal is buffering, stalled or bursting —
/// not drifting. Such a window is discarded and the estimate stays put.
const PLAUSIBLE_PPM: f64 = 200_000.0;
/// The estimate is an exponentially weighted average of the windows.
const GAIN: f64 = 0.25;
/// `Locked` once this many consecutive windows barely move the estimate.
const SETTLE_WINDOWS: u32 = 3;
/// A window is "settled" when it moves the estimate by less than this.
const SETTLE_PPM: f64 = 25.0;

/// What one consumer's drift measurement currently says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriftReport {
    /// Estimated clock drift of the consumer against the timeline, parts
    /// per million: positive means the consumer eats samples faster than
    /// nominal and its feed must be stretched.
    pub drift_ppm: f64,
    pub state: SyncState,
}

/// Tracks one consumer's drift and derives the resampler correction.
///
/// The estimate converges on the consumer's true rate error through noisy
/// windows and the applied ratio tracks it, so steering moves the ratio in
/// the direction the consumer is actually off by and settles there.
#[derive(Debug)]
pub struct DriftController {
    nominal_hz: f64,
    /// Cumulative consumed frames and the instant they were reported at.
    last: Option<(u64, Instant)>,
    estimate_ppm: f64,
    seeded: bool,
    settled_windows: u32,
    windows: u32,
}

impl DriftController {
    /// `nominal_hz` is the rate the consumer should be running at (its
    /// output sample rate).
    pub fn new(nominal_hz: u32) -> Self {
        DriftController {
            nominal_hz: f64::from(nominal_hz),
            last: None,
            estimate_ppm: 0.0,
            seeded: false,
            settled_windows: 0,
            windows: 0,
        }
    }

    /// Report that the consumer has accepted `frames_total` frames in total,
    /// at instant `now`. Call as often as convenient; windows shorter than
    /// [`MIN_WINDOW`] are merged, not measured.
    pub fn observe(&mut self, frames_total: u64, now: Instant) {
        let Some((last_frames, last_at)) = self.last else {
            self.last = Some((frames_total, now));
            return;
        };
        let elapsed = now.duration_since(last_at).as_secs_f64();
        if elapsed < MIN_WINDOW.as_secs_f64() {
            // Keep the older anchor so short reports widen one window.
            return;
        }
        self.last = Some((frames_total, now));
        let frames = frames_total.saturating_sub(last_frames) as f64;
        let measured_ppm = (frames / elapsed / self.nominal_hz - 1.0) * 1e6;
        if !measured_ppm.is_finite() || measured_ppm.abs() > PLAUSIBLE_PPM {
            // A burst (client filling its buffer), a stall, or a clock jump.
            // The next window starts fresh; the estimate is not dragged.
            self.settled_windows = 0;
            return;
        }
        self.windows += 1;
        if !self.seeded {
            // First real window: start from what was observed rather than
            // from zero, so steering points the right way immediately.
            self.estimate_ppm = measured_ppm;
            self.seeded = true;
            self.settled_windows = 0;
            return;
        }
        let delta = measured_ppm - self.estimate_ppm;
        let movement = GAIN * delta;
        self.estimate_ppm += movement;
        if movement.abs() <= SETTLE_PPM {
            self.settled_windows += 1;
        } else {
            self.settled_windows = 0;
        }
    }

    /// The current drift estimate in ppm, once at least one real window has
    /// been measured.
    pub fn drift_ppm(&self) -> Option<f64> {
        self.seeded.then_some(self.estimate_ppm)
    }

    /// The resampler correction to apply, ppm of the nominal ratio. Always
    /// the current estimate; the resampler clamps its own bound.
    pub fn correction_ppm(&self) -> f64 {
        self.estimate_ppm
    }

    /// Coarse state: `Syncing` while the estimate is still moving,
    /// `Locked` once consecutive windows agree with it. A burst or a
    /// diverging window knocks a locked consumer back to `Syncing`.
    pub fn state(&self) -> SyncState {
        if self.settled_windows >= SETTLE_WINDOWS {
            SyncState::Locked
        } else {
            SyncState::Syncing
        }
    }

    pub fn report(&self) -> Option<DriftReport> {
        self.drift_ppm().map(|drift_ppm| DriftReport {
            drift_ppm,
            state: self.state(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `windows` one-second observations of a consumer running at
    /// `drift_ppm` off nominal, with the given measurement noise.
    fn observe_consumer(drift_ppm: f64, noise_ppm: f64, windows: usize) -> DriftController {
        let nominal_hz = 44_100u32;
        let mut controller = DriftController::new(nominal_hz);
        let base = Instant::now();
        let mut frames = 0u64;
        controller.observe(0, base);
        for window in 0..windows {
            let noise = if noise_ppm == 0.0 {
                0.0
            } else {
                noise_ppm * ((window as f64 * 1.7).sin())
            };
            frames += (f64::from(nominal_hz) * (1.0 + (drift_ppm + noise) / 1e6)).round() as u64;
            controller.observe(frames, base + Duration::from_secs(window as u64 + 1));
        }
        controller
    }

    #[test]
    fn steering_moves_the_estimate_the_way_the_consumer_actually_drifts() {
        let fast = observe_consumer(500.0, 0.0, 5);
        assert!(
            fast.correction_ppm() > 400.0,
            "a consumer 500 ppm fast must push the ratio up, got {}",
            fast.correction_ppm()
        );
        let slow = observe_consumer(-300.0, 0.0, 5);
        assert!(
            slow.correction_ppm() < -250.0,
            "a consumer 300 ppm slow must pull the ratio down, got {}",
            slow.correction_ppm()
        );
    }

    #[test]
    fn the_estimate_converges_through_noisy_windows_and_locks() {
        let controller = observe_consumer(500.0, 40.0, 40);
        let drift = controller.drift_ppm().expect("measured");
        assert!(
            (drift - 500.0).abs() < 25.0,
            "estimate {drift} must converge on the true 500 ppm"
        );
        assert_eq!(controller.state(), SyncState::Locked);
    }

    #[test]
    fn a_burst_is_not_a_rate_measurement() {
        let mut controller = DriftController::new(44_100);
        let base = Instant::now();
        controller.observe(0, base);
        // One second of a client filling its buffer at ten times nominal.
        controller.observe(441_000, base + Duration::from_secs(1));
        assert_eq!(controller.drift_ppm(), None, "the burst must be discarded");
        // A real window after it seeds normally.
        controller.observe(485_100, base + Duration::from_secs(2));
        let drift = controller.drift_ppm().expect("measured");
        assert!(
            (drift - 0.0).abs() < 1.0,
            "second window is nominal, got {drift}"
        );
    }
}
