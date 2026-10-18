//! The per-transport playout latency model.
//!
//! Every transport buffers audio for some time between the bytes leaving
//! this machine and sound coming out of the speaker. A sync group delays
//! faster transports so that every member presents the timeline at the same
//! wall-clock instant (see [`crate::sync`]).
//!
//! A latency figure is one of two things, and the model never blurs them:
//!
//! - a **measurement** — what [`crate::calibration`]'s mic loopback
//!   actually recorded on the user's machine. Its source is kept in
//!   [`on_air_api::CalibrationMeasurement`] and it sets `calibrated: true`;
//! - a **documented default** — an estimate, marked `calibrated: false`,
//!   corrected at most by the user's trim (`PUT /api/sync/trim`). A trim
//!   is manual, by ear: it is NOT a measurement and never flips
//!   `calibrated` on its own.
//!
//! The defaults (all unmeasured where no measurement exists):
//! - AirPlay ~2000 ms: the ~2 s AirPlay2 buffer the design spec recorded.
//! - Bluetooth ~150 ms: the middle of the 100–300 ms range the design spec
//!   recorded for A2DP plus OS sink buffering.
//! - Local Pulse 50 ms: the `--latency-msec=50` the local path passes to
//!   `pacat`.
//! - Sonos 1000 ms: an UNMEASURED placeholder. Sonos buffers the live HTTP
//!   radio stream by an amount nobody has characterized; this figure
//!   exists so the delay-line arithmetic has something to hold onto, and
//!   it is wrong until measured.
//!
//! The design spec (`docs/design/2026-08-26-on-air-design.md`) insists
//! timing must be verified acoustically by mic loopback and never trusted
//! from reported API latency — that is what the calibration harness is
//! for, and this module is where its results replace the guesses.

use std::time::Duration;

/// One transport's playout latency as the sync machinery sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyEstimate {
    pub latency: Duration,
    /// True only when the base figure is an acoustic measurement
    /// ([`crate::calibration`]). A documented default is never
    /// calibrated; a user trim applied on top of either stays a manual
    /// correction and is reported separately (`trim_ms` on the wire).
    pub calibrated: bool,
}

/// The documented default latency per transport, in milliseconds. Used
/// only where no measurement exists.
pub fn default_latency_ms(transport: &str) -> u32 {
    match transport {
        "airplay" => 2_000,
        "bluetooth" => 150,
        "local" => 50,
        "sonos" => 1_000,
        _ => 0,
    }
}

/// The user's trim range, in milliseconds either way.
pub const TRIM_LIMIT_MS: i32 = 10_000;

/// The effective latency in microseconds: the measured value when one has
/// been applied, else the documented default, plus the user's trim, never
/// negative.
pub fn effective_latency_us(
    transport: &str,
    measured_us: Option<u64>,
    trim_ms: i32,
) -> u64 {
    let base = measured_us.unwrap_or(u64::from(default_latency_ms(transport)) * 1_000);
    let trimmed = base as i64 + i64::from(trim_ms) * 1_000;
    trimmed.clamp(0, i64::from(u32::MAX) * 1_000) as u64
}

/// The effective latency in milliseconds (the wire figure, rounded to the
/// nearest millisecond).
pub fn effective_latency_ms(transport: &str, measured_us: Option<u64>, trim_ms: i32) -> u32 {
    let us = effective_latency_us(transport, measured_us, trim_ms);
    ((us + 500) / 1_000).min(u64::from(u32::MAX)) as u32
}

/// The estimate a sync group uses for one member.
pub fn effective_latency(
    transport: &str,
    measured_us: Option<u64>,
    trim_ms: i32,
) -> LatencyEstimate {
    LatencyEstimate {
        latency: Duration::from_micros(effective_latency_us(transport, measured_us, trim_ms)),
        calibrated: measured_us.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_documented_per_transport_and_never_calibrated() {
        assert_eq!(default_latency_ms("airplay"), 2_000);
        assert_eq!(default_latency_ms("bluetooth"), 150);
        assert_eq!(default_latency_ms("local"), 50);
        assert_eq!(default_latency_ms("sonos"), 1_000);
        let estimate = effective_latency("sonos", None, 0);
        assert!(!estimate.calibrated, "no default is a measurement");
        assert_eq!(estimate.latency, Duration::from_millis(1_000));
    }

    #[test]
    fn a_measurement_replaces_the_default_and_is_calibrated() {
        let estimate = effective_latency("sonos", Some(987_654), 0);
        assert!(estimate.calibrated, "a measurement is a measurement");
        assert_eq!(estimate.latency, Duration::from_micros(987_654));
        // The 1000 ms Sonos placeholder is gone entirely.
        assert_ne!(estimate.latency, Duration::from_millis(1_000));
    }

    #[test]
    fn trims_shift_the_base_but_never_go_negative() {
        assert_eq!(effective_latency_ms("bluetooth", None, 250), 400);
        assert_eq!(effective_latency_ms("bluetooth", None, -100), 50);
        assert_eq!(effective_latency_ms("bluetooth", None, -5_000), 0);
        // A trim also nudges a measured value; the result stays
        // calibrated because its base is still the measurement.
        assert_eq!(effective_latency_ms("bluetooth", Some(150_000), -100), 50);
        let estimate = effective_latency("bluetooth", Some(150_000), -100);
        assert!(estimate.calibrated);
    }

    #[test]
    fn measured_figures_keep_their_sub_millisecond_resolution() {
        let estimate = effective_latency("airplay", Some(1_987_432), 0);
        assert_eq!(estimate.latency, Duration::from_micros(1_987_432));
        assert_eq!(effective_latency_ms("airplay", Some(1_987_432), 0), 1_987);
    }
}
