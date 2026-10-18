//! DSP quality profile: selects the resampler on every conversion path.
//!
//! The wire spelling (`economy`/`balanced`/`high`) and the profile
//! semantics match the shared `on_air_api::DspQuality` enum that PR #17
//! introduces; this core-local definition exists because that crate is
//! outside this change's slice. When the two trees merge, this type should
//! collapse into a re-export of the shared one — nothing else changes.
//!
//! - `economy`: the cubic Farrow polynomial interpolator. Cheapest; this
//!   is what every on-air resampler used before quality profiles existed.
//! - `balanced`: windowed sinc interpolation, sinc length 128.
//! - `high`: windowed sinc interpolation, sinc length 256 — a larger
//!   filter table and worse cache behaviour on old hardware for a modest
//!   quality gain over `balanced`.
//!
//! The per-profile costs those docs imply are not promises: the audio
//! check measures each profile on the machine that will run it
//! ([`crate::audio_benchmark::run`]) and the recommendation is decided
//! from that.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DspQuality {
    Economy,
    Balanced,
    High,
}

impl DspQuality {
    /// Every selectable profile, cheapest first.
    pub const ALL: [DspQuality; 3] = [DspQuality::Economy, DspQuality::Balanced, DspQuality::High];
}

impl Default for DspQuality {
    /// Economy: a machine that never runs the audio check pays no more CPU
    /// than it did before quality profiles existed.
    fn default() -> Self {
        DspQuality::Economy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_wire_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&DspQuality::Balanced).unwrap(),
            "\"balanced\""
        );
        assert_eq!(
            serde_json::from_str::<DspQuality>("\"high\"").unwrap(),
            DspQuality::High
        );
    }
}
