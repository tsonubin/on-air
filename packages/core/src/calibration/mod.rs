//! Acoustic latency calibration (roadmap B10): measure, do not guess.
//!
//! The per-transport latency defaults in [`crate::sync::latency`] are
//! documented guesses. This module replaces a guess with a measurement on
//! the user's own machine, in their own room, against their own speakers:
//!
//! 1. A known probe ([`probe`], a swept sine — see its docs for why) is
//!    published on the pipeline PCM bus in the exact [`PcmChunk`] format
//!    the processing task publishes. Every sender and the per-receiver
//!    radio stream (`/stream/{nonce}/audio.wav`) carry it through the
//!    **existing** transport path — Sonos, AirPlay, Bluetooth, local/Pulse
//!    — unchanged. There is no second audio path.
//! 2. A microphone ([`record`]) listens while the probe plays. The
//!    recorder uses the same capture backends the product captures with
//!    (`pipeline::capture`: `parec` on Pulse/PipeWire, cpal elsewhere).
//! 3. The arrival is found by cross-correlation ([`detect`]), per probe
//!    run, and the runs are aggregated into one figure with an honest
//!    confidence ([`aggregate`], below).
//!
//! # What the measured number is, exactly
//!
//! The reported latency is the **loopback delay** from the moment a probe
//! frame is published on the PCM bus to the moment its sound arrives at
//! the microphone. Beyond the transport's own buffering it therefore
//! includes two terms software cannot separate from the inside:
//!
//! - **speaker → microphone propagation**, ~3 ms per metre;
//! - **the capture path's delivery latency**: samples reach the process
//!   up to one capture buffer after they were recorded (bounded by the
//!   recorder's `--latency-msec` / cpal buffer setting).
//!
//! Both terms are *constant* for every transport measured on one machine
//! with one mic position, so the differences between transports — exactly
//! what a sync group aligns — are unaffected by them. Any residual
//! absolute offset is what `PUT /api/sync/trim` is for. The estimator's
//! own error is far below these terms; see [`detect`] for its floors and
//! the synthetic tests for the verified accuracy.
//!
//! # Confidence
//!
//! Every run yields a correlation peak with a prominence (peak over the
//! correlation's own sidelobes) and a normalized match. A run is trusted
//! only when both clear [`PROMINENCE_USABLE`] / [`CORRELATION_USABLE`];
//! noise alone never does (the detector's tests pin the separation). The
//! aggregate is [`CalibrationConfidence::Low`] — reported as bad and
//! **never applied** to the sync machinery — when fewer than half the
//! runs were trustworthy or the runs disagree by more than
//! [`SPREAD_BAD_US`]. Anything the harness cannot stand behind is
//! reported as bad rather than trusted.
//!
//! [`PcmChunk`]: crate::timeline::PcmChunk
//! [`CalibrationConfidence::Low`]: on_air_api::CalibrationConfidence::Low

pub mod detect;
pub mod probe;
pub mod record;
pub mod run;

use on_air_api::{CalibrationConfidence, CalibrationMeasurement};

/// A correlation peak at least this many times above the correlation's own
/// sidelobes is a real arrival. Noise-only peaks sit near 3–6 (the
/// detector's tests pin that); a probe run through any real channel sits
/// far above. 12 leaves clear margin on both sides.
pub const PROMINENCE_USABLE: f64 = 12.0;
/// Above this, with tight run-to-run agreement, a measurement is `High`.
pub const PROMINENCE_HIGH: f64 = 30.0;
/// The normalized match at the peak must clear this for a run to be
/// trusted. Scale-invariant; noise-only sits below 0.05.
pub const CORRELATION_USABLE: f64 = 0.2;
/// Runs agreeing within this (µs) make a measurement `High`.
pub const SPREAD_GOOD_US: u64 = 5_000;
/// Runs disagreeing worse than this (µs) make it `Low`: the number is not
/// repeatable, so it must not replace a default.
pub const SPREAD_BAD_US: u64 = 50_000;

/// One probe run's detection, as aggregation sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunOutcome {
    /// Measured loopback delay in microseconds. May be slightly negative
    /// from timing jitter; aggregation rejects those runs as implausible.
    pub delay_us: i64,
    pub prominence: f64,
    pub correlation: f64,
    /// Capture samples this run's window contained (what the figure is
    /// "based on").
    pub samples: u64,
}

/// Fold one target's probe runs into a measurement.
///
/// Returns `None` when no run produced a trustworthy arrival: nothing was
/// measured, and the caller must say so rather than invent a figure. When
/// runs are trustworthy but marginal, the measurement is still built —
/// with [`CalibrationConfidence::Low`], which the caller reports and does
/// not apply. `measured_at_unix_s` stamps the completion.
pub fn aggregate(
    transport: &str,
    device_id: &str,
    runs: &[RunOutcome],
    sample_rate_hz: u32,
    measured_at_unix_s: u64,
) -> Option<CalibrationMeasurement> {
    let trusted: Vec<&RunOutcome> = runs
        .iter()
        .filter(|run| {
            run.delay_us >= 0
                && run.prominence >= PROMINENCE_USABLE
                && run.correlation >= CORRELATION_USABLE
        })
        .collect();
    if trusted.is_empty() {
        return None;
    }

    let mut delays: Vec<i64> = trusted.iter().map(|run| run.delay_us).collect();
    delays.sort_unstable();
    let median = delays[delays.len() / 2];
    let spread = (*delays.last().unwrap() - delays[0]).max(0) as u64;
    let samples = trusted.iter().map(|run| run.samples).sum();
    let min_prominence = trusted
        .iter()
        .map(|run| run.prominence)
        .fold(f64::INFINITY, f64::min);

    let confidence = if trusted.len() < runs.len().div_ceil(2) || spread > SPREAD_BAD_US {
        CalibrationConfidence::Low
    } else if trusted.len() == runs.len()
        && min_prominence >= PROMINENCE_HIGH
        && spread <= SPREAD_GOOD_US
    {
        CalibrationConfidence::High
    } else {
        CalibrationConfidence::Medium
    };

    Some(CalibrationMeasurement {
        transport: transport.to_string(),
        device_id: device_id.to_string(),
        latency_us: median.max(0) as u64,
        confidence,
        runs: trusted.len() as u32,
        samples,
        spread_us: spread,
        sample_rate_hz,
        measured_at_unix_s,
        run_delays_us: delays.iter().map(|d| *d as u64).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(delay_us: i64, prominence: f64) -> RunOutcome {
        RunOutcome {
            delay_us,
            prominence,
            correlation: 0.9,
            samples: 24_000,
        }
    }

    #[test]
    fn consistent_clean_runs_aggregate_to_a_high_confidence_measurement() {
        let runs = [
            run(150_250, 400.0),
            run(150_260, 380.0),
            run(150_240, 410.0),
        ];
        let m = aggregate("bluetooth", "bt-1", &runs, 48_000, 1_760_000_000).unwrap();
        assert_eq!(m.latency_us, 150_250);
        assert_eq!(m.confidence, CalibrationConfidence::High);
        assert_eq!(m.runs, 3);
        assert_eq!(m.samples, 72_000);
        assert_eq!(m.spread_us, 20);
        assert_eq!(m.run_delays_us, vec![150_240, 150_250, 150_260]);
    }

    #[test]
    fn noise_only_runs_are_reported_as_unmeasured_not_as_a_number() {
        let runs = [run(31_415, 4.0), run(27_182, 3.5), run(16_180, 5.0)];
        assert_eq!(
            aggregate("sonos", "s-1", &runs, 48_000, 0),
            None,
            "no trustworthy arrival: no figure is produced at all"
        );
    }

    #[test]
    fn one_good_run_among_bad_ones_is_low_confidence_and_must_not_be_applied() {
        let runs = [
            run(150_250, 400.0),
            run(71_000, 4.0),
            run(233_000, 5.0),
        ];
        let m = aggregate("bluetooth", "bt-1", &runs, 48_000, 0).unwrap();
        assert_eq!(m.confidence, CalibrationConfidence::Low);
        assert_eq!(m.runs, 1);
    }

    #[test]
    fn runs_that_disagree_badly_are_low_confidence() {
        let runs = [run(150_000, 400.0), run(220_000, 350.0), run(90_000, 320.0)];
        let m = aggregate("airplay", "ap-1", &runs, 48_000, 0).unwrap();
        assert_eq!(m.confidence, CalibrationConfidence::Low);
    }

    #[test]
    fn marginal_prominence_is_medium_not_high() {
        let runs = [run(150_250, 15.0), run(150_260, 14.0), run(150_255, 16.0)];
        let m = aggregate("local", "local", &runs, 48_000, 0).unwrap();
        assert_eq!(m.confidence, CalibrationConfidence::Medium);
    }

    #[test]
    fn implausible_negative_runs_are_rejected() {
        let runs = [run(-25_000, 400.0), run(-24_000, 400.0), run(150_250, 400.0)];
        let m = aggregate("bluetooth", "bt-1", &runs, 48_000, 0).unwrap();
        assert_eq!(m.runs, 1);
        assert_eq!(m.latency_us, 150_250);
    }
}
