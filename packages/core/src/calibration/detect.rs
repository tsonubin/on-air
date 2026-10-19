//! Delay detection by cross-correlation (matched filtering) between the
//! probe as played and the probe as captured.
//!
//! The estimator is deliberately classical:
//!
//! 1. **Cross-correlate** the capture with the played probe. For a swept
//!    sine the correlation is the sweep's autocorrelation (a sharp peak of
//!    width ~1/bandwidth, ~0.18 ms for the default sweep) displaced to the
//!    arrival delay, convolved with the acoustic path. Additive noise
//!    spreads into flat sidelobes; the peak-to-sidelobe ratio is the
//!    detector's confidence.
//! 2. **Refine the peak below one sample**: parabolic interpolation for a
//!    starting guess, then the peak of the band-limited (windowed-sinc)
//!    interpolant of the correlation in ±1 sample at 1/64-sample steps.
//!    This is exact reconstruction of the correlation between samples, so
//!    the reported delay carries fractional samples — not a quantised
//!    integer.
//! 3. **Score the peak**: `prominence` is the peak over the RMS of all
//!    other correlation lags (a real arrival stands far above the
//!    correlation's own noise floor); `correlation` is the normalized
//!    match (Pearson-style, 0..1) between the capture and the reference at
//!    the peak. Both are scale-invariant, so an attenuated signal is not
//!    penalized as long as it is still *there*.
//!
//! # What the number means and its floors
//!
//! The raw correlation lives on the capture sample grid: one sample is
//! 22.7 µs at 44.1 kHz, 20.8 µs at 48 kHz, 10.4 µs at 96 kHz. That is the
//! **quantisation floor of an unrefined peak pick**. The sinc refinement
//! reports the band-limited peak position continuously; the synthetic
//! tests in this module pin its error orders of magnitude below one sample
//! under clean conditions and below one millisecond under noise,
//! attenuation and speaker colouration. Real hardware adds what software
//! cannot see — see [`crate::calibration`] for the loopback budget.

use std::time::Duration;

/// One detected arrival.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Arrival delay in capture-rate samples, fractional.
    pub delay_samples: f64,
    /// The same delay in microseconds at the capture rate.
    pub delay_us: u64,
    /// Peak over the RMS of every non-peak correlation lag: how far the
    /// arrival stands above the correlation's own floor. ~3–6 for noise
    /// alone; tens to thousands for a real probe.
    pub prominence: f64,
    /// Normalized match at the peak, 0.0..=1.0 (1.0 = the capture is the
    /// reference, scaled). Scale-invariant.
    pub correlation: f64,
}

/// Why nothing could be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DetectError {
    #[error("capture is shorter than the probe")]
    TooShort,
    #[error("no signal in the capture (silence)")]
    NoSignal,
}

/// Sinc-interpolation taps around the peak (each side). 24 taps truncates
/// the ideal kernel at ~-40 dB; plenty for peak *position*.
const SINC_TAPS: usize = 24;
/// Refinement grid inside ±1 sample.
const REFINE_STEPS: i32 = 64;

/// Correlate `capture` against `reference`, both mono, at `rate_hz`, and
/// find the arrival of the reference inside the capture. Lags beyond
/// `max_delay` are never considered, so a distant echo cannot win.
pub fn detect_delay(
    reference: &[f32],
    capture: &[f32],
    rate_hz: u32,
    max_delay: Duration,
) -> Result<Detection, DetectError> {
    let n = reference.len();
    let m = capture.len();
    if n < 16 {
        return Err(DetectError::TooShort);
    }
    if m < n {
        return Err(DetectError::TooShort);
    }
    let max_lag = ((max_delay.as_secs_f64() * f64::from(rate_hz)) as usize).min(m - n);
    if max_lag == 0 {
        return Err(DetectError::TooShort);
    }

    let corr = cross_correlation(reference, capture);
    // Only fully-overlapping lags: partial overlap shrinks the sum and
    // would bias both the peak search and the sidelobe statistics.
    let lags = &corr[..=max_lag];
    let mut peak_lag = 0usize;
    let mut peak = 0.0f32;
    for (lag, value) in lags.iter().enumerate() {
        if value.abs() > peak {
            peak = value.abs();
            peak_lag = lag;
        }
    }
    if peak <= f32::EPSILON {
        return Err(DetectError::NoSignal);
    }

    // Sidelobe statistics: every lag except the peak's mainlobe. The
    // mainlobe half-width of a swept sine is ~2/bandwidth seconds.
    let mainlobe = (2.5 * f64::from(rate_hz) / PROBE_BANDWIDTH_HZ).ceil() as usize + 2;
    let mut energy = 0.0f64;
    let mut count = 0usize;
    for (lag, value) in lags.iter().enumerate() {
        if lag.abs_diff(peak_lag) <= mainlobe {
            continue;
        }
        energy += f64::from(*value) * f64::from(*value);
        count += 1;
    }
    let rms = (energy / count.max(1) as f64).sqrt();
    let prominence = f64::from(peak) / rms.max(f64::from(peak) * 1e-12);

    // Normalized match at the peak (polarity-insensitive).
    let mut ref_energy = 0.0f64;
    for s in reference {
        ref_energy += f64::from(*s) * f64::from(*s);
    }
    let mut cap_energy = 0.0f64;
    for s in &capture[peak_lag..(peak_lag + n).min(m)] {
        cap_energy += f64::from(*s) * f64::from(*s);
    }
    let norm = (ref_energy * cap_energy).sqrt();
    let correlation = if norm > 0.0 {
        (f64::from(corr[peak_lag]) / norm).abs().min(1.0)
    } else {
        0.0
    };

    let delay_samples = refine_peak(&corr, peak_lag) as f64;
    Ok(Detection {
        delay_samples,
        delay_us: (delay_samples * 1_000_000.0 / f64::from(rate_hz)).round() as u64,
        prominence,
        correlation,
    })
}

/// The bandwidth of the default probe, used to size the peak's mainlobe.
/// Kept in step with [`super::probe::ProbeSpec::default`] so the sidelobe
/// statistics never cut into the peak itself.
const PROBE_BANDWIDTH_HZ: f64 = 5_700.0;

/// Zero-padded FFT cross-correlation: `out[l] = Σ_k capture[k+l]·ref[k]`
/// for every non-wrapping lag. One allocation pair per call — this runs
/// offline on a finished recording, never on the audio thread.
fn cross_correlation(reference: &[f32], capture: &[f32]) -> Vec<f32> {
    let n = reference.len();
    let m = capture.len();
    let len = (m + n).next_power_of_two();
    let mut cap = vec![0.0f32; len];
    cap[..m].copy_from_slice(capture);
    let mut refr = vec![0.0f32; len];
    refr[..n].copy_from_slice(reference);

    let mut planner = realfft::RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(len);
    let ifft = planner.plan_fft_inverse(len);
    let mut cap_spec = fft.make_output_vec();
    let mut ref_spec = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    fft.process_with_scratch(&mut cap, &mut cap_spec, &mut scratch)
        .expect("spectrum sizes match");
    fft.process_with_scratch(&mut refr, &mut ref_spec, &mut scratch)
        .expect("spectrum sizes match");
    for (c, r) in cap_spec.iter_mut().zip(ref_spec.iter()) {
        *c *= r.conj();
    }
    let mut out = vec![0.0f32; len];
    let mut scratch = ifft.make_scratch_vec();
    ifft.process_with_scratch(&mut cap_spec, &mut out, &mut scratch)
        .expect("spectrum sizes match");
    let scale = 1.0 / len as f32;
    for value in &mut out {
        *value *= scale;
    }
    out
}

/// Sub-sample peak position: parabolic guess, then the maximum of the
/// band-limited (sinc) interpolant of `corr` in ±1 sample around it.
fn refine_peak(corr: &[f32], peak_lag: usize) -> f32 {
    if peak_lag == 0 || peak_lag + 1 >= corr.len() {
        return peak_lag as f32;
    }
    let (y0, y1, y2) = (
        f64::from(corr[peak_lag - 1]),
        f64::from(corr[peak_lag]),
        f64::from(corr[peak_lag + 1]),
    );
    let parabolic = {
        let denom = y0 - 2.0 * y1 + y2;
        if denom.abs() < f64::EPSILON {
            0.0
        } else {
            (0.5 * (y0 - y2) / denom).clamp(-1.0, 1.0)
        }
    };

    // Evaluate the sinc interpolant on a 1/64 grid over ±1 sample and
    // refine around its best point with a parabola over grid neighbours.
    let eval = |delta: f64| -> f64 {
        let tau = peak_lag as f64 + delta;
        let lo = tau.ceil() as i64 - SINC_TAPS as i64;
        let hi = tau.floor() as i64 + SINC_TAPS as i64;
        let mut sum = 0.0;
        for j in lo..=hi {
            if j < 0 || j as usize >= corr.len() {
                continue;
            }
            let x = tau - j as f64;
            sum += f64::from(corr[j as usize]) * sinc(x);
        }
        sum
    };
    let mut best = parabolic;
    let mut best_value = eval(parabolic);
    let step = 1.0 / REFINE_STEPS as f64;
    for i in -REFINE_STEPS..=REFINE_STEPS {
        let delta = i as f64 * step;
        let value = eval(delta);
        if value > best_value {
            best_value = value;
            best = delta;
        }
    }
    let left = eval(best - step);
    let right = eval(best + step);
    let denom = left - 2.0 * best_value + right;
    let refined = if denom.abs() < f64::EPSILON {
        best
    } else {
        best + 0.5 * step * (left - right) / denom
    };
    (peak_lag as f64 + refined) as f32
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
    }
}

/// Band-limited (sinc) shift of `samples` right by a fractional
/// `shift_samples`, keeping every sample (`out` is `shift` longer). This
/// is the same interpolation the peak refinement evaluates, used to
/// inject synthetic arrivals at exact fractional delays — in the detector
/// tests and in mock mode's loopback.
pub fn shift_bandlimited(samples: &[f32], shift_samples: f64) -> Vec<f32> {
    let n = samples.len();
    let out_len = n + (shift_samples.ceil().max(0.0)) as usize;
    let mut out = vec![0.0f32; out_len];
    for (i, value) in out.iter_mut().enumerate() {
        let t = i as f64 - shift_samples;
        let lo = t.ceil() as i64 - SINC_TAPS as i64;
        let hi = t.floor() as i64 + SINC_TAPS as i64;
        let mut sum = 0.0;
        for j in lo..=hi {
            if j < 0 || j as usize >= n {
                continue;
            }
            sum += f64::from(samples[j as usize]) * sinc(t - j as f64);
        }
        *value = sum as f32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::probe::ProbeSpec;

    /// Render the probe at `rate`, then shift it by a fractional `delay_s`
    /// with sinc interpolation (band-limited, exactly like a real sampled
    /// path with an arbitrary physical delay).
    fn delayed_probe(spec: &ProbeSpec, rate: u32, delay_s: f64, total_s: f64) -> Vec<f32> {
        let rendered = spec.render(rate);
        let shifted = shift_bandlimited(&rendered, delay_s * f64::from(rate));
        let total = (total_s * f64::from(rate)).ceil() as usize;
        let mut out = vec![0.0f32; total];
        let copied = shifted.len().min(total);
        out[..copied].copy_from_slice(&shifted[..copied]);
        out
    }

    fn expected_us(delay_s: f64) -> f64 {
        delay_s * 1_000_000.0
    }

    #[test]
    fn injected_delays_are_recovered_at_every_rate() {
        let spec = ProbeSpec::default();
        // 50 ms, 150 ms and 2 s — the real-transport neighbourhood (local,
        // Bluetooth, AirPlay) — at three capture rates, both whole and
        // fractional.
        let mut worst_us = 0.0f64;
        for rate in [44_100u32, 48_000, 96_000] {
            for delay_s in [0.050, 0.150, 2.0, 0.150_253, 2.000_041] {
                let capture = delayed_probe(&spec, rate, delay_s, delay_s + 1.0);
                let reference = spec.render(rate);
                let found = detect_delay(&reference, &capture, rate, Duration::from_millis(2_500))
                    .unwrap_or_else(|e| panic!("{rate} Hz / {delay_s} s: {e}"));
                let error_us = (found.delay_us as f64 - expected_us(delay_s)).abs();
                worst_us = worst_us.max(error_us);
                assert!(
                    error_us < 5.0,
                    "{rate} Hz / {delay_s} s: error {error_us} µs (got {} µs)",
                    found.delay_us
                );
                assert!(found.prominence > 30.0, "clean input stands out");
                assert!(found.correlation > 0.9, "clean input matches");
            }
        }
        println!("worst clean-input delay error across rates/delays: {worst_us} µs");
    }

    #[test]
    fn the_refinement_beats_the_sample_grid() {
        // A delay exactly between two samples: an integer peak pick cannot
        // do better than half a sample (10.4 µs at 48 kHz), the refinement
        // must.
        let spec = ProbeSpec::default();
        let rate = 48_000u32;
        let delay_s = 0.150_000 + 0.5 / f64::from(rate);
        let capture = delayed_probe(&spec, rate, delay_s, 0.7);
        let reference = spec.render(rate);
        let found = detect_delay(&reference, &capture, rate, Duration::from_millis(500)).unwrap();
        let error_us = (found.delay_us as f64 - expected_us(delay_s)).abs();
        assert!(error_us < 5.0, "half-sample delay error {error_us} µs");
    }

    /// Degradation suite: noise, attenuation, and a linear-phase FIR
    /// standing in for speaker colouration. The FIR's own group delay is
    /// folded into the expected arrival (it is known exactly).
    #[test]
    fn noisy_attenuated_and_coloured_input_is_still_recovered() {
        let spec = ProbeSpec::default();
        let rate = 48_000u32;
        let delay_s = 0.150_253;
        let clean = delayed_probe(&spec, rate, delay_s, 0.9);

        // Attenuation to -26 dB and 0 dB SNR white noise.
        let mut noisy = Vec::with_capacity(clean.len());
        let mut seed = 0x243F6A8885A308D3u64;
        for s in &clean {
            // xorshift64*, deterministic so the test cannot flake.
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let uniform = (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            let noise = uniform * 0.007;
            noisy.push(*s * 0.05 + noise as f32);
        }

        // Linear-phase FIR colouration: a bandpass tilt with a dip, the
        // kind of response a real speaker has. Symmetric taps => constant
        // group delay of (taps-1)/2 samples, folded into the expectation.
        let taps: Vec<f32> = vec![0.1, -0.25, 0.6, 1.2, 0.6, -0.25, 0.1];
        let group_delay_s = (taps.len() - 1) as f64 / 2.0 / f64::from(rate);
        let mut coloured = vec![0.0f32; noisy.len()];
        for i in taps.len() / 2..noisy.len() - taps.len() / 2 {
            let mut acc = 0.0;
            for (k, tap) in taps.iter().enumerate() {
                acc += tap * noisy[i + k - taps.len() / 2];
            }
            coloured[i] = acc;
        }

        let reference = spec.render(rate);
        for (name, capture) in [("noise+attenuation", &noisy), ("+ colouration", &coloured)] {
            let found = detect_delay(&reference, capture, rate, Duration::from_millis(500))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let want = expected_us(delay_s + group_delay_s);
            let error_us = (found.delay_us as f64 - want).abs();
            println!(
                "{name}: error {error_us} µs (prominence {:.1})",
                found.prominence
            );
            assert!(error_us < 1_000.0, "{name}: error {error_us} µs");
            assert!(
                found.prominence > 12.0 && found.correlation > 0.2,
                "{name}: a degraded but real arrival is still trusted \
                 (prominence {}, correlation {})",
                found.prominence,
                found.correlation
            );
        }
    }

    #[test]
    fn noise_alone_never_looks_like_an_arrival() {
        let spec = ProbeSpec::default();
        let rate = 48_000u32;
        let reference = spec.render(rate);
        let mut seed = 0x9E3779B97F4A7C15u64;
        let noise: Vec<f32> = (0..reference.len() * 3)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5) as f32
            })
            .collect();
        let found = detect_delay(&reference, &noise, rate, Duration::from_millis(500))
            .expect("noise produces a peak candidate, not an error");
        assert!(
            found.prominence < 12.0,
            "noise-only prominence must stay below the trust threshold, got {}",
            found.prominence
        );
        assert!(
            found.correlation < 0.2,
            "noise-only correlation must stay low, got {}",
            found.correlation
        );
    }

    #[test]
    fn silence_is_reported_as_no_signal() {
        let spec = ProbeSpec::default();
        let rate = 48_000u32;
        let reference = spec.render(rate);
        let capture = vec![0.0f32; reference.len() * 2];
        assert_eq!(
            detect_delay(&reference, &capture, rate, Duration::from_millis(500)),
            Err(DetectError::NoSignal)
        );
    }

    #[test]
    fn a_capture_shorter_than_the_probe_is_an_error_not_a_guess() {
        let spec = ProbeSpec::default();
        let rate = 48_000u32;
        let reference = spec.render(rate);
        let capture = vec![0.1f32; reference.len() / 2];
        assert_eq!(
            detect_delay(&reference, &capture, rate, Duration::from_millis(500)),
            Err(DetectError::TooShort)
        );
    }
}
