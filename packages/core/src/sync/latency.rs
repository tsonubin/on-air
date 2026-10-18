//! The per-transport playout latency model.
//!
//! Every transport buffers audio for some time between the bytes leaving
//! this machine and sound coming out of the speaker. A sync group delays
//! faster transports so that every member presents the timeline at the same
//! wall-clock instant (see [`crate::sync`]).
//!
//! The numbers here are DOCUMENTED DEFAULTS, not measurements. The design
//! spec (`docs/design/2026-08-26-on-air-design.md`) insists that timing must
//! be verified acoustically by mic loopback and never trusted from reported
//! API latency, and nothing on this host has been measured: there is no
//! hardware here. Every estimate is therefore marked `calibrated: false`
//! until the user trims it against what they actually hear.

use std::time::Duration;

/// One transport's playout latency as the sync machinery sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyEstimate {
    pub latency: Duration,
    /// True only for a figure calibrated by measurement. Documented
    /// defaults are never calibrated; a user trim is not a measurement
    /// either, it is a manual correction.
    pub calibrated: bool,
}

/// A documented default latency plus the user's trim in milliseconds.
///
/// Starting defaults (all unmeasured, all `calibrated: false`):
/// - AirPlay ~2000 ms: the ~2 s AirPlay2 buffer the design spec recorded.
/// - Bluetooth ~150 ms: the middle of the 100–300 ms range the design spec
///   recorded for A2DP plus OS sink buffering.
/// - Local Pulse 50 ms: the `--latency-msec=50` the local path passes to
///   `pacat`.
/// - Sonos 1000 ms: an UNMEASURED placeholder. Sonos buffers the live HTTP
///   radio stream by an amount nobody has characterized on this host; this
///   figure exists so the delay-line arithmetic has something to hold onto,
///   and it is wrong until measured. The trim control is the escape hatch.
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

/// Effective latency: the documented default plus the user's trim, never
/// negative. A trim is manual calibration, so the result stays uncalibrated.
pub fn effective_latency_ms(transport: &str, trim_ms: i32) -> u32 {
    let default = i64::from(default_latency_ms(transport));
    let trimmed = default + i64::from(trim_ms);
    trimmed.clamp(0, i64::from(u32::MAX)) as u32
}

/// The estimate a sync group uses for one member.
pub fn effective_latency(transport: &str, trim_ms: i32) -> LatencyEstimate {
    LatencyEstimate {
        latency: Duration::from_millis(u64::from(effective_latency_ms(transport, trim_ms))),
        calibrated: false,
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
        let estimate = effective_latency("sonos", 0);
        assert!(!estimate.calibrated, "no default is a measurement");
        assert_eq!(estimate.latency, Duration::from_millis(1_000));
    }

    #[test]
    fn trims_shift_the_default_but_never_go_negative() {
        assert_eq!(effective_latency_ms("bluetooth", 250), 400);
        assert_eq!(effective_latency_ms("bluetooth", -100), 50);
        assert_eq!(effective_latency_ms("bluetooth", -5_000), 0);
    }
}
