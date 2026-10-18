//! The core's JSON shapes, from the shared `on-air-api` crate. The core
//! serializes exactly these types, and `tests/mock_core.rs` drives the
//! client against the real core in mock mode.
//!
//! The first-run tuning shapes are mirrored from the core's
//! `api::audio_performance` / `settings` structs instead of the shared
//! crate (which is outside the change that added them); the mock-core
//! tests check the copies against the real serde structs. `DspQuality` is
//! the shared crate's enum, re-exported above: its local mirror collapsed
//! into that re-export when the quality-profile and first-run-tuning trees
//! merged.

pub use on_air_api::*;

use serde::{Deserialize, Serialize};

/// What the automatic first-run tuning measured and did. Mirrors the
/// core's `settings::FirstRunTuning`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstRunTuningRecord {
    pub ran_at_unix_s: u64,
    pub recommended_buffer_ms: u32,
    pub applied_buffer: bool,
    pub applied_quality: bool,
}

/// `GET /api/audio/tuning`. Mirrors the core's
/// `api::audio_performance::TuningStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuningStatus {
    pub tuned: bool,
    pub automatic: Option<FirstRunTuningRecord>,
    pub quality: DspQuality,
    pub quality_override: Option<DspQuality>,
    pub recommended_quality: Option<DspQuality>,
    pub capture_buffer_ms: u32,
    pub profiles: Vec<DspQuality>,
}
