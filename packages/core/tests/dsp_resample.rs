use on_air_core::dsp::resample::MonoResampler;

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

    assert_eq!(out.len(), input.len());
    let max_diff = input
        .iter()
        .zip(out.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_diff < 0.02, "max_diff was {max_diff}");
}
