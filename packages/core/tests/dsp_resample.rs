use on_air_core::dsp::resample::{MonoResampler, StreamingMonoResampler};

#[test]
fn upsamples_44100_to_48000_with_correct_ratio() {
    let mut r = MonoResampler::new(44100, 48000);
    let n = r.input_frames_next();
    let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
    let out = r.process(&input);

    assert!(!out.is_empty());
    let ratio = out.len() as f64 / n as f64;
    assert!((ratio - 48000.0 / 44100.0).abs() < 0.05, "ratio was {ratio}");
}

#[test]
fn identity_ratio_is_near_lossless() {
    let mut r = MonoResampler::new(44100, 44100);
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
    let mut r = StreamingMonoResampler::new(48_000, 48_000);
    let input = vec![0.1, -0.2, 0.3];
    assert_eq!(r.process(&input), input);
}

#[test]
fn streaming_resampler_accepts_uneven_chunks_44100_to_48000() {
    let mut r = StreamingMonoResampler::new(44_100, 48_000);
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
