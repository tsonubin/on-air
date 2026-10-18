use on_air_core::dsp::resample::{PcmResampler, StreamingResampler};
use on_air_core::dsp::DspQuality;

/// One-shot wrapper for the allocation-free `process_into` API.
fn pcm(r: &mut PcmResampler, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::new();
    r.process_into(input, &mut out);
    out
}

fn streaming(r: &mut StreamingResampler, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::new();
    r.process_into(input, &mut out);
    out
}

#[test]
fn upsamples_44100_to_48000_with_correct_ratio() {
    for quality in DspQuality::ALL {
        let mut r = PcmResampler::new(44100, 48000, 1, quality);
        let n = r.input_frames_next();
        let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = pcm(&mut r, &input);

        assert!(!out.is_empty());
        let ratio = out.len() as f64 / n as f64;
        assert!(
            (ratio - 48000.0 / 44100.0).abs() < 0.05,
            "ratio was {ratio} at {quality:?}"
        );
    }
}

#[test]
fn identity_ratio_reproduces_the_signal_after_warm_up() {
    for quality in DspQuality::ALL {
        let mut r = PcmResampler::new(44100, 44100, 1, quality);
        let n = r.input_frames_next();
        // The same chunk every call: the input stream is periodic with
        // period `n`, so the steady-state output is the input delayed by
        // the filter's constant group delay.
        let sine: Vec<f32> = (0..n).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let mut out = Vec::new();
        for _ in 0..8 {
            r.process_into(&sine, &mut out);
        }
        assert!(
            out.len() > n / 2,
            "identity path produced {} frames from {n} at {quality:?}",
            out.len()
        );
        // Find the delay and require the waveform to survive unchanged:
        // the identity path must not colour, damp or clip the signal.
        let m = out.len().min(n);
        let mut best = f32::MAX;
        for lag in 0..m {
            let worst = (0..m - lag)
                .map(|i| (out[i + lag] - sine[i]).abs())
                .fold(0.0f32, f32::max);
            best = best.min(worst);
        }
        assert!(
            best < 0.02,
            "identity path distorted the signal at {quality:?} (best max_diff {best})"
        );
    }
}

#[test]
fn streaming_passthrough_keeps_samples_when_rates_match() {
    let mut r = StreamingResampler::new(48_000, 48_000, 1, DspQuality::Economy);
    let input = vec![0.1, -0.2, 0.3];
    assert_eq!(streaming(&mut r, &input), input);
}

#[test]
fn streaming_resampler_accepts_uneven_chunks_44100_to_48000() {
    for quality in DspQuality::ALL {
        let mut r = StreamingResampler::new(44_100, 48_000, 1, quality);
        let mut output = Vec::new();
        // Inner resampler wants 1024-frame chunks; leftover frames stay buffered.
        let fed = 200 * 12;
        for _ in 0..12 {
            output.extend(streaming(&mut r, &[0.0; 200]));
        }
        assert!(
            !output.is_empty(),
            "expected resampled frames from buffered chunks at {quality:?}"
        );
        let consumed = (fed / 1024) * 1024;
        let ratio = output.len() as f64 / consumed as f64;
        assert!(
            (ratio - 48_000.0 / 44_100.0).abs() < 0.15,
            "ratio was {ratio} (out={}, consumed={consumed}) at {quality:?}",
            output.len()
        );
    }
}

const PI: f32 = std::f32::consts::PI;

#[test]
fn stereo_resample_keeps_channels_apart() {
    for quality in DspQuality::ALL {
        let mut r = PcmResampler::new(44_100, 48_000, 2, quality);
        let frames = r.input_frames_next();
        let tone = |n: usize| (2.0 * PI * 1000.0 * n as f32 / 44_100.0).sin() * 0.5;
        let mut out = Vec::new();
        for c in 0..20 {
            let chunk: Vec<f32> = (0..frames)
                .flat_map(|i| [tone(c * frames + i), 0.0])
                .collect();
            out.extend(pcm(&mut r, &chunk));
        }
        assert_eq!(out.len() % 2, 0);
        let (left, right): (Vec<f32>, Vec<f32>) =
            out.as_chunks::<2>().0.iter().map(|[l, r]| (*l, *r)).unzip();
        assert!(
            right.iter().all(|s| s.abs() < 1e-6),
            "right picked up the left tone at {quality:?}"
        );
        assert!(left.iter().skip(256).any(|s| s.abs() > 0.4));
        let ratio = left.len() as f64 / (frames * 20) as f64;
        assert!(
            (ratio - 48_000.0 / 44_100.0).abs() < 0.02,
            "ratio {ratio} at {quality:?}"
        );
    }
}

#[test]
#[should_panic(expected = "input_frames_next")]
fn stereo_chunk_must_hold_both_channels() {
    let mut r = PcmResampler::new(44_100, 48_000, 2, DspQuality::Economy);
    let mono_sized = vec![0.0; r.input_frames_next()];
    pcm(&mut r, &mono_sized);
}

#[test]
fn streaming_stereo_accepts_any_whole_frame_count() {
    for quality in DspQuality::ALL {
        let mut r = StreamingResampler::new(44_100, 48_000, 2, quality);
        let mut out = Vec::new();
        for _ in 0..8 {
            out.extend(streaming(&mut r, &[0.25, -0.25].repeat(333)));
        }
        assert!(!out.is_empty());
        assert_eq!(out.len() % 2, 0);
    }
}

#[test]
fn same_rate_streaming_stereo_is_lossless() {
    let mut r = StreamingResampler::new(48_000, 48_000, 2, DspQuality::High);
    assert_eq!(
        streaming(&mut r, &[0.1, -0.1, 0.2, -0.2]),
        vec![0.1, -0.1, 0.2, -0.2]
    );
}
