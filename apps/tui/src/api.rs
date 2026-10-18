//! The core's JSON shapes, from the shared `on-air-api` crate. The core
//! serializes exactly these types, and `tests/mock_core.rs` drives the
//! client against the real core in mock mode.
//!
//! The first-run tuning shapes are mirrored from the core's
//! `api::audio_performance` / `settings` structs instead of the shared
//! crate (which is outside the change that added them); the mock-core
//! tests check the copies against the real serde structs.

pub use on_air_api::*;

use serde::{Deserialize, Serialize};

/// DSP quality profile: selects the resampler on every conversion path
/// (`economy`: cubic Farrow; `balanced`/`high`: windowed sinc at increasing
/// filter lengths). Mirrors the core's `dsp::DspQuality`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DspQuality {
    /// The default: no more CPU than before quality profiles existed.
    #[default]
    Economy,
    Balanced,
    High,
}

impl DspQuality {
    /// Every selectable profile, cheapest first.
    pub const ALL: [DspQuality; 3] = [DspQuality::Economy, DspQuality::Balanced, DspQuality::High];

    /// The next profile in the cycle (economy → balanced → high → …).
    pub fn next(self) -> Self {
        match self {
            DspQuality::Economy => DspQuality::Balanced,
            DspQuality::Balanced => DspQuality::High,
            DspQuality::High => DspQuality::Economy,
        }
    }
}

impl std::fmt::Display for DspQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            DspQuality::Economy => "economy",
            DspQuality::Balanced => "balanced",
            DspQuality::High => "high",
        };
        f.write_str(name)
    }
}

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

/// `PUT /api/audio/quality`. Mirrors the core's
/// `api::audio_performance::SetDspQualityRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetDspQualityRequest {
    pub quality: DspQuality,
}
