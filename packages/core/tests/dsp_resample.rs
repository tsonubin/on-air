use on_air_core::dsp::resample::{PcmResampler, StreamingResampler};
use on_air_core::dsp::DspQuality;

#[test]
fn upsamples_44100_to_48000_with_correct_ratio() {
    let mut r = PcmResampler::new(44100, 48000, 1, DspQuality::Economy);
    let n = r.input_frames_next();
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
    let out = r.process(&input);

    assert!(!out.is_empty());
    let ratio = out.len() as f64 / n as f64;
    assert!(
        (ratio - 48000.0 / 44100.0).abs() < 0.05,
        "ratio was {ratio}"
    );
}

#[test]
fn identity_ratio_is_near_lossless() {
    let mut r = PcmResampler::new(44100, 44100, 1, DspQuality::Economy);
    let n = r.input_frames_next();
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
    let out = r.process(&input);

    // rubato's async resampler with FixedAsync::Input has internal filter-delay/warm-up
    // buffering, so it doesn't emit exactly N output frames for the first N-frame chunk.
    // Allow output length within 10 frames of input (measured: 1021 vs 1024).
    let len_diff = (out.len() as i64 - input.len() as i64).unsigned_abs();
    assert!(
        len_diff <= 10,
        "expected output length near {}, got {} (diff {len_diff})",
        input.len(),
        out.len()
    );
    let max_diff = input
        .iter()
        .zip(out.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_diff < 0.02, "max_diff was {max_diff}");
}

#[test]
fn streaming_passthrough_keeps_samples_when_rates_match() {
    let mut r = StreamingResampler::new(48_000, 48_000, 1, DspQuality::Economy);
    let input = vec![0.1, -0.2, 0.3];
    assert_eq!(r.process(&input), input);
}

#[test]
fn streaming_resampler_accepts_uneven_chunks_44100_to_48000() {
    let mut r = StreamingResampler::new(44_100, 48_000, 1, DspQuality::Economy);
    let mut output = Vec::new();
    // Inner resampler wants 1024-frame chunks; leftover frames stay buffered.
    let fed = 200 * 12;
    for _ in 0..12 {
        output.extend(r.process(&[0.0; 200]));
    }
    assert!(
        !output.is_empty(),
        "expected resampled frames from buffered chunks"
    );
    let consumed = (fed / 1024) * 1024;
    let ratio = output.len() as f64 / consumed as f64;
    assert!(
        (ratio - 48_000.0 / 44_100.0).abs() < 0.15,
        "ratio was {ratio} (out={}, consumed={consumed})",
        output.len()
    );
}

const PI: f32 = std::f32::consts::PI;

#[test]
fn stereo_resample_keeps_channels_apart() {
    let mut r = PcmResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let frames = r.input_frames_next();
    let tone = |n: usize| (2.0 * PI * 1000.0 * n as f32 / 44_100.0).sin() * 0.5;
    let mut out = Vec::new();
    for c in 0..20 {
        let chunk: Vec<f32> = (0..frames)
            .flat_map(|i| [tone(c * frames + i), 0.0])
            .collect();
        out.extend(r.process(&chunk));
    }
    assert_eq!(out.len() % 2, 0);
    let (left, right): (Vec<f32>, Vec<f32>) =
        out.as_chunks::<2>().0.iter().map(|[l, r]| (*l, *r)).unzip();
    assert!(
        right.iter().all(|s| s.abs() < 1e-6),
        "right picked up the left tone"
    );
    assert!(left.iter().skip(256).any(|s| s.abs() > 0.4));
    let ratio = left.len() as f64 / (frames * 20) as f64;
    assert!((ratio - 48_000.0 / 44_100.0).abs() < 0.02, "ratio {ratio}");
}

#[test]
#[should_panic(expected = "input_frames_next")]
fn stereo_chunk_must_hold_both_channels() {
    let mut r = PcmResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let mono_sized = vec![0.0; r.input_frames_next()];
    r.process(&mono_sized);
}

#[test]
fn streaming_stereo_accepts_any_whole_frame_count() {
    let mut r = StreamingResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let mut out = Vec::new();
    for _ in 0..8 {
        out.extend(r.process(&[0.25, -0.25].repeat(333)));
    }
    assert!(!out.is_empty());
    assert_eq!(out.len() % 2, 0);
}

#[test]
fn same_rate_streaming_stereo_is_lossless() {
    let mut r = StreamingResampler::new(48_000, 48_000, 2, DspQuality::Economy);
    assert_eq!(
        r.process(&[0.1, -0.1, 0.2, -0.2]),
        vec![0.1, -0.1, 0.2, -0.2]
    );
}

#[test]
fn each_quality_selects_a_different_resampler() {
    let economy = PcmResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let balanced = PcmResampler::new(44_100, 48_000, 2, DspQuality::Balanced);
    let high = PcmResampler::new(44_100, 48_000, 2, DspQuality::High);
    // Economy is the cubic Farrow polynomial interpolator: no sinc
    // anti-aliasing filter — exactly the resampler every path used before
    // quality profiles existed. The tests above pin its behaviour while
    // running at Economy; rubato confirms the interpolation kind here.
    assert_eq!(economy.cutoff(), None, "economy is the polynomial path");
    let balanced_cutoff = balanced.cutoff().expect("balanced is a sinc resampler");
    let high_cutoff = high.cutoff().expect("high is a sinc resampler");
    assert!(
        high_cutoff > balanced_cutoff,
        "the 256-tap sinc (high) derives a cutoff closer to Nyquist than the 128-tap one \
         (balanced): {high_cutoff} vs {balanced_cutoff}"
    );
}

#[test]
fn the_quality_profiles_resample_a_near_nyquist_tone_differently() {
    fn resample(quality: DspQuality) -> Vec<f32> {
        let mut r = PcmResampler::new(44_100, 48_000, 1, quality);
        let n = r.input_frames_next();
        let tone = |i: usize| (2.0 * PI * 17_000.0 * i as f32 / 44_100.0).sin() * 0.5;
        let mut out = Vec::new();
        for chunk in 0..8 {
            let input: Vec<f32> = (0..n).map(|i| tone(chunk * n + i)).collect();
            out.extend(r.process(&input));
        }
        out
    }
    // Steady state only: the warm-up window is filter delay, not response.
    let skip = 2048;
    let max_diff = |a: &[f32], b: &[f32]| {
        let len = a.len().min(b.len());
        (skip..len)
            .map(|i| (a[i] - b[i]).abs())
            .fold(0.0f32, f32::max)
    };
    let economy = resample(DspQuality::Economy);
    let balanced = resample(DspQuality::Balanced);
    let high = resample(DspQuality::High);
    for (name, a, b) in [
        ("economy vs balanced", &economy, &balanced),
        ("economy vs high", &economy, &high),
        ("balanced vs high", &balanced, &high),
    ] {
        let diff = max_diff(a, b);
        assert!(
            diff > 0.01,
            "the {name} profiles must interpolate differently; max diff was {diff}"
        );
    }
}

#[test]
fn the_steerable_resampler_follows_the_same_profile_ladder() {
    use on_air_core::dsp::resample::SteerableResampler;
    let economy = SteerableResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let balanced = SteerableResampler::new(44_100, 48_000, 2, DspQuality::Balanced);
    let high = SteerableResampler::new(44_100, 48_000, 2, DspQuality::High);
    assert_eq!(economy.cutoff(), None);
    let balanced_cutoff = balanced.cutoff().expect("balanced is a sinc resampler");
    let high_cutoff = high.cutoff().expect("high is a sinc resampler");
    assert!(high_cutoff > balanced_cutoff);
}

/// Documentation aid, not a gate: prints the measured steady-state cost of
/// one 1024-frame stereo chunk through each profile on this machine.
/// Run with `cargo test -p on-air-core --test dsp_resample -- --ignored --nocapture`.
#[test]
#[ignore = "prints measurements; run by hand for the PR's cost table"]
fn report_per_profile_resampler_costs() {
    use std::time::Instant;
    let frames = 1024 * 64;
    let tone = |i: usize| (2.0 * PI * 1000.0 * i as f32 / 44_100.0).sin() * 0.5;
    let input: Vec<f32> = (0..frames).map(tone).collect();
    for quality in DspQuality::ALL {
        let mut r = PcmResampler::new(44_100, 48_000, 2, quality);
        let n = r.input_frames_next();
        let mut out = Vec::new();
        let started = Instant::now();
        for chunk in input.chunks_exact(n) {
            let stereo: Vec<f32> = chunk.iter().flat_map(|s| [*s, *s]).collect();
            out.extend(r.process(&stereo));
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let per_chunk = elapsed / (frames / n) as f64;
        println!("{quality:?}: {per_chunk:.4} ms per {n}-frame chunk");
    }
}
