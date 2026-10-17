use on_air_core::dsp::eq::{GraphicEq, EQ_BAND_CENTERS_HZ, EQ_GAIN_RANGE_DB};

#[test]
fn zero_gain_is_exact_passthrough() {
    let mut eq = GraphicEq::new(44100.0, 1);
    let input: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.017).sin() * 0.6).collect();
    let mut samples = input.clone();
    eq.process(&mut samples);
    for (a, b) in input.iter().zip(samples.iter()) {
        // Tolerance accounts for float32 accumulation error through 5 cascaded Direct-Form-I biquads (measured ~1.22e-4), with 8x safety margin.
        assert!((a - b).abs() < 1e-3, "expected passthrough, got {a} vs {b}");
    }
}

#[test]
fn set_gains_db_clamps_to_range() {
    let mut eq = GraphicEq::new(44100.0, 1);
    eq.set_gains_db([20.0, -20.0, 0.0, 0.0, 0.0]);
    let gains = eq.gains_db();
    assert_eq!(gains[0], EQ_GAIN_RANGE_DB.1);
    assert_eq!(gains[1], EQ_GAIN_RANGE_DB.0);
}

#[test]
fn non_finite_gains_are_treated_as_flat() {
    let mut eq = GraphicEq::new(44100.0, 1);
    eq.set_gains_db([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 3.0, 0.0]);
    assert_eq!(eq.gains_db(), [0.0, 0.0, 0.0, 3.0, 0.0]);

    let mut samples: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.017).sin() * 0.6).collect();
    eq.process(&mut samples);
    assert!(
        samples.iter().all(|s| s.is_finite()),
        "a non-finite gain must never poison the audio path"
    );
}

#[test]
fn boosting_a_band_amplifies_its_center_frequency() {
    let sample_rate = 44100.0;
    let band_index = 2; // 1000 Hz
    let freq = EQ_BAND_CENTERS_HZ[band_index];

    let mut gains = [0.0; 5];
    gains[band_index] = 12.0;

    let n = 4000;
    let sine: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
        .collect();

    let mut boosted = sine.clone();
    let mut eq = GraphicEq::new(sample_rate, 1);
    eq.set_gains_db(gains);
    eq.process(&mut boosted);

    // skip the filter's settling transient, compare steady-state RMS
    let settle = n / 4;
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
    let ratio = rms(&boosted[settle..]) / rms(&sine[settle..]);

    // +12dB amplitude ratio is 10^(12/20) ~= 3.98
    assert!(ratio > 3.0 && ratio < 4.5, "ratio was {ratio}");
}

#[test]
fn stereo_eq_keeps_channel_state_apart() {
    let gains = [6.0, -3.0, 4.0, 0.0, 2.0];
    let left: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.017).sin() * 0.6).collect();

    let mut mono = GraphicEq::new(48_000.0, 1);
    mono.set_gains_db(gains);
    let mut expected = left.clone();
    mono.process(&mut expected);

    let mut stereo = GraphicEq::new(48_000.0, 2);
    stereo.set_gains_db(gains);
    let mut interleaved: Vec<f32> = left.iter().flat_map(|&l| [l, 0.0]).collect();
    stereo.process(&mut interleaved);

    let (l, r): (Vec<f32>, Vec<f32>) = interleaved
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[l, r]| (*l, *r))
        .unzip();
    assert_eq!(l, expected, "left must match a mono EQ bit for bit");
    assert!(
        r.iter().all(|&s| s == 0.0),
        "right picked up left's filter state"
    );
}
