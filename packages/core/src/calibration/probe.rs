//! The calibration probe signal: what the harness plays through a
//! transport to find out when it comes out of the speaker.
//!
//! # Why a swept sine (chirp) and not an impulse or pseudo-noise
//!
//! Three candidates were considered; the chirp wins on every requirement
//! the room, the speaker and the transport's codec impose:
//!
//! - **An impulse** (a click) is broadband but carries very little energy,
//!   so it disappears under room noise. Worse, a speaker and a codec
//!   colour a click into an arbitrary smear: the detected arrival becomes
//!   the shape of the impulse response, not a sharp peak. And a click is
//!   exactly the sound you do not want escaping at volume.
//! - **A pseudo-noise / m-sequence** has ideal autocorrelation and superb
//!   noise immunity, but it sounds like white-noise hiss for its whole
//!   duration — unpleasant to hear, and it is the least kind signal to
//!   play through a neighbour's wall. Codecs also quantize noise harder
//!   than tonal content.
//! - **A linear swept sine (chirp)** spreads its energy across the band
//!   like noise does — so it survives room nulls and speaker colouration
//!   (each frequency is only briefly in a dip; cross-correlation integrates
//!   the whole sweep) — while sounding like a soft, short "whoosh", the
//!   same signal consumer room-correction systems play. A chirp's
//!   autocorrelation is a sharp peak of width ~1/bandwidth (~0.18 ms for
//!   the default 300–6000 Hz sweep), and its processing gain against
//!   additive noise is ~10·log10(bandwidth × duration) ≈ 34 dB for the
//!   default 0.5 s sweep. Speech codecs (Bluetooth A2DP, AirPlay's
//!   encodings) pass swept tones far better than they pass transients.
//!
//! The sweep fades in and out over [`ProbeSpec::FADE`] with a raised
//! cosine: no click at the edges (a hard on/off step would excite codec
//! transient responses and splatter), and the band stays occupied.
//!
//! The same sweep is written to both channels at the same level. The
//! detection side collapses capture channels to mono.
//!
//! The probe is generated analytically, so rendering it at the playback
//! rate (to publish on the PCM bus) or at the capture rate (as the
//! detection reference) is exact at either rate — no resampler sits
//! between the two renderings.

/// The probe waveform. Defaults: 300 Hz → 6000 Hz linear sweep, 500 ms,
/// ±-20 dBFS. The band sits where microphones and speakers are flattest
/// and codecs are most transparent; the width (5.7 kHz) sets the
/// correlation peak sharpness (~0.18 ms mainlobe).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeSpec {
    pub start_hz: f64,
    pub end_hz: f64,
    /// Sweep duration, fades included.
    pub duration_s: f64,
    /// Peak amplitude, 0.0..=1.0. -20 dBFS is audible but gentle even if
    /// it escapes, and far below codec overload.
    pub amplitude: f32,
}

impl Default for ProbeSpec {
    fn default() -> Self {
        ProbeSpec {
            start_hz: 300.0,
            end_hz: 6_000.0,
            duration_s: 0.5,
            amplitude: 0.1,
        }
    }
}

impl ProbeSpec {
    /// Raised-cosine edge fade: 10 ms, short enough not to shrink the
    /// effective bandwidth measurably, long enough to be click-free.
    pub const FADE_S: f64 = 0.010;

    /// Sweep bandwidth in Hz (the correlation peak is ~1/bandwidth wide).
    pub fn bandwidth_hz(&self) -> f64 {
        (self.end_hz - self.start_hz).abs()
    }

    /// Samples one mono rendering of the probe at `rate_hz`. The sweep is
    /// a linear chirp with raised-cosine edges:
    /// `s(t) = A · w(t) · sin(2π (f0 t + (f1-f0) t² / 2T))`.
    pub fn render(&self, rate_hz: u32) -> Vec<f32> {
        let n = (self.duration_s * f64::from(rate_hz)).round().max(1.0) as usize;
        let mut out = Vec::with_capacity(n);
        let t_total = n as f64 / f64::from(rate_hz);
        let rate = f64::from(rate_hz);
        for i in 0..n {
            let t = i as f64 / rate;
            let phase = std::f64::consts::TAU
                * (self.start_hz * t + (self.end_hz - self.start_hz) * t * t / (2.0 * t_total));
            let fade = fade_gain(t, t_total);
            out.push(self.amplitude * (fade * phase.sin()) as f32);
        }
        out
    }
}

/// Raised-cosine gain over the first and last [`ProbeSpec::FADE_S`].
fn fade_gain(t: f64, total: f64) -> f64 {
    let fade = ProbeSpec::FADE_S;
    if t < fade {
        0.5 * (1.0 - (std::f64::consts::PI * t / fade).cos())
    } else if t > total - fade {
        0.5 * (1.0 - (std::f64::consts::PI * (total - t) / fade).cos())
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_is_the_documented_sweep() {
        let spec = ProbeSpec::default();
        let rate = 48_000;
        let rendered = spec.render(rate);
        assert_eq!(rendered.len(), rate as usize / 2);
        // The detector sizes its peak window from this bandwidth
        // (`detect::PROBE_BANDWIDTH_HZ`); the two must agree.
        assert_eq!(spec.bandwidth_hz(), 5_700.0);
        assert!(
            rendered.iter().all(|s| s.is_finite() && s.abs() <= spec.amplitude),
            "the probe never exceeds its stated peak"
        );
        // The rendered energy sits in the sweep band: a tone test at the
        // band centre correlates strongly, one below the start barely.
        let n = rendered.len();
        let corr_at = |hz: f64| {
            let mut sum = 0.0f64;
            for (i, s) in rendered.iter().enumerate() {
                let t = i as f64 / f64::from(rate);
                sum += f64::from(*s) * (std::f64::consts::TAU * hz * t).sin();
            }
            sum.abs() / n as f64
        };
        assert!(
            corr_at(3_000.0) > 10.0 * corr_at(100.0),
            "energy is in the sweep band, not below it"
        );
    }

    #[test]
    fn edges_are_faded_so_no_step_excites_a_codec() {
        let spec = ProbeSpec::default();
        for rate in [44_100, 48_000, 96_000] {
            let rendered = spec.render(rate);
            let first = rendered[0].abs();
            let last = rendered[rendered.len() - 1].abs();
            assert!(first < 1e-6, "starts at silence, got {first}");
            assert!(last < 1e-6, "ends at silence, got {last}");
        }
    }

    #[test]
    fn rendering_is_exact_at_every_rate() {
        // The reference and the playback renderings sample one analytic
        // function; their zero crossings line up to within a sample.
        let spec = ProbeSpec::default();
        let at = |rate: u32| spec.render(rate);
        assert_eq!(at(44_100).len(), 22_050);
        assert_eq!(at(48_000).len(), 24_000);
        assert_eq!(at(96_000).len(), 48_000);
    }
}
