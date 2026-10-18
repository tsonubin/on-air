use on_air_core::dsp::channel::{ChannelMode, ChannelPlan, Destination, SinkConstruction};
use on_air_core::dsp::resample::{PcmResampler, StreamingResampler};

/// A one-channel plan, as only an explicit mono mode (or a genuinely mono
/// destination) can justify one (roadmap A4/A6).
fn mono_plan() -> ChannelPlan {
    SinkConstruction::ProcessingThread
        .plan(ChannelMode::Mono, Destination::stereo("resample test"))
        .unwrap()
}

/// The pipeline's own stereo layout.
fn stereo_plan() -> ChannelPlan {
    SinkConstruction::ProcessingThread.auto(Destination::stereo("resample test"))
}

#[test]
fn upsamples_44100_to_48000_with_correct_ratio() {
    let mut r = PcmResampler::new(44100, 48000, &mono_plan());
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
    let mut r = PcmResampler::new(44100, 44100, &mono_plan());
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
    let mut r = StreamingResampler::new(48_000, 48_000, &mono_plan());
    let input = vec![0.1, -0.2, 0.3];
    assert_eq!(r.process(&input), input);
}

#[test]
fn streaming_resampler_accepts_uneven_chunks_44100_to_48000() {
    let mut r = StreamingResampler::new(44_100, 48_000, &mono_plan());
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
    let mut r = PcmResampler::new(44_100, 48_000, &stereo_plan());
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
    let mut r = PcmResampler::new(44_100, 48_000, &stereo_plan());
    let mono_sized = vec![0.0; r.input_frames_next()];
    r.process(&mono_sized);
}

#[test]
fn streaming_stereo_accepts_any_whole_frame_count() {
    let mut r = StreamingResampler::new(44_100, 48_000, &stereo_plan());
    let mut out = Vec::new();
    for _ in 0..8 {
        out.extend(r.process(&[0.25, -0.25].repeat(333)));
    }
    assert!(!out.is_empty());
    assert_eq!(out.len() % 2, 0);
}

#[test]
fn same_rate_streaming_stereo_is_lossless() {
    let mut r = StreamingResampler::new(48_000, 48_000, &stereo_plan());
    assert_eq!(
        r.process(&[0.1, -0.1, 0.2, -0.2]),
        vec![0.1, -0.1, 0.2, -0.2]
    );
}
