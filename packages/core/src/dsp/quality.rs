//! DSP quality profile: selects the resampler on every conversion path.
//!
//! - `economy`: the cubic Farrow polynomial interpolator. Cheapest; this
//!   is what every on-air resampler used before quality profiles existed.
//! - `balanced`: windowed sinc interpolation, sinc length 128.
//! - `high`: windowed sinc interpolation, sinc length 256 — a larger
//!   filter table and worse cache behaviour on old hardware for a modest
//!   quality gain over `balanced`.
//!
//! The profile enum is the shared `on_air_api::DspQuality`, re-exported as
//! [`crate::dsp::DspQuality`]: this core-local definition collapsed into
//! that re-export when the quality-profile and first-run-tuning trees
//! merged. The per-profile costs those docs imply are not promises: the
//! audio check measures each profile on the machine that will run it
//! ([`crate::audio_benchmark::run`]) and the recommendation is decided
//! from that.

#[cfg(test)]
mod tests {
    use crate::dsp::DspQuality;

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
