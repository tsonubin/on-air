use super::resample::StreamingResampler;
use super::{DspQuality, PIPELINE_CHANNELS};

/// Decode pipeline stereo L16 LE, resample to the output rate, then fit it to
/// the destination channel count. This is the sender-side half of the
/// input/output sample-rate bridge.
///
/// `quality` selects the resampler interpolation profile at construction.
/// The `*_into` conversions keep their scratch inside the bridge and hand
/// output to a caller-owned buffer: they allocate nothing per chunk in
/// steady state (the C7 allocation guard proves it), so callers must reuse
/// their output buffer across chunks.
pub struct RateBridge {
    resampler: StreamingResampler,
    channels: usize,
    /// Scratch: decoded f32 input.
    decoded: Vec<f32>,
    /// Scratch: resampled stereo.
    resampled: Vec<f32>,
    /// Scratch: channel-mapped output for the L16 encode path.
    mapped: Vec<f32>,
}

impl RateBridge {
    pub fn new(
        input_rate_hz: u32,
        output_rate_hz: u32,
        channels: usize,
        quality: DspQuality,
    ) -> Self {
        RateBridge {
            resampler: StreamingResampler::new(
                input_rate_hz,
                output_rate_hz,
                PIPELINE_CHANNELS,
                quality,
            ),
            channels: channels.max(1),
            decoded: Vec::new(),
            resampled: Vec::new(),
            mapped: Vec::new(),
        }
    }

    /// Decodes, resamples and channel-maps `pcm` into `out`, which is
    /// cleared and filled with this call's PCM.
    pub fn process_l16_into(&mut self, pcm: &[u8], out: &mut Vec<f32>) {
        let Self {
            resampler,
            channels,
            decoded,
            resampled,
            ..
        } = self;
        l16_le_to_f32_into(pcm, decoded);
        resampler.process_into(decoded, resampled);
        map_stereo_into(resampled, *channels, out);
    }

    /// Like [`RateBridge::process_l16_into`], ending at L16 LE for the wire.
    pub fn process_l16_to_l16_into(&mut self, pcm: &[u8], out: &mut Vec<u8>) {
        let Self {
            resampler,
            channels,
            decoded,
            resampled,
            mapped,
        } = self;
        l16_le_to_f32_into(pcm, decoded);
        resampler.process_into(decoded, resampled);
        map_stereo_into(resampled, *channels, mapped);
        f32_to_l16_le_into(mapped, out);
    }

    pub fn output_rate_hz(&self) -> u32 {
        self.resampler.output_rate_hz()
    }

    pub fn channels(&self) -> usize {
        self.channels
    }
}

/// Interleaved stereo L16 LE → interleaved f32, appended to `out` after it
/// is cleared. A trailing partial frame is dropped.
pub fn l16_le_to_f32_into(pcm: &[u8], out: &mut Vec<f32>) {
    let (samples, _) = pcm.as_chunks::<2>();
    out.clear();
    out.reserve(samples.len());
    out.extend(
        samples
            .iter()
            .map(|sample| i16::from_le_bytes(*sample) as f32 / i16::MAX as f32),
    );
}

/// Interleaved f32 → interleaved stereo L16 LE (clamped to full scale),
/// written to `out`, which is cleared first.
pub fn f32_to_l16_le_into(samples: &[f32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * 2);
    for &s in samples {
        let sample = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }
}

/// Fits interleaved pipeline stereo to a destination with `channels`
/// channels: one channel gets the average of left and right, two pass
/// through, and any further channels get silence. Written to `out`, which
/// is cleared first and must not alias `samples`.
pub fn map_stereo_into(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    let channels = channels.max(1);
    out.clear();
    out.reserve(samples.len() / PIPELINE_CHANNELS * channels);
    if channels == PIPELINE_CHANNELS {
        out.extend_from_slice(samples);
        return;
    }
    let (frames, _) = samples.as_chunks::<PIPELINE_CHANNELS>();
    for frame in frames {
        if channels == 1 {
            out.push((frame[0] + frame[1]) * 0.5);
        } else {
            out.extend_from_slice(frame);
            out.extend(std::iter::repeat_n(0.0, channels - PIPELINE_CHANNELS));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_stereo_fits_the_destination_layout() {
        let mut out = Vec::new();
        map_stereo_into(&[0.5, 0.25], 1, &mut out);
        assert_eq!(out, vec![0.375]);
        map_stereo_into(&[0.5, 0.25], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.25]);
        map_stereo_into(&[0.5, 0.25], 4, &mut out);
        assert_eq!(out, vec![0.5, 0.25, 0.0, 0.0]);
    }

    fn l16(samples: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        f32_to_l16_le_into(samples, &mut out);
        out
    }

    #[test]
    fn same_rate_bridge_keeps_left_and_right() {
        let mut bridge = RateBridge::new(44_100, 44_100, 2, DspQuality::Economy);
        let mut out = Vec::new();
        bridge.process_l16_into(&l16(&[0.25, -0.25]), &mut out);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.25).abs() < 0.01 && (out[1] + 0.25).abs() < 0.01);
    }

    #[test]
    fn mono_destination_hears_both_sides() {
        let mut bridge = RateBridge::new(44_100, 44_100, 1, DspQuality::Economy);
        let mut out = Vec::new();
        bridge.process_l16_into(&l16(&[0.5, 0.0]), &mut out);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.25).abs() < 0.01);
    }

    #[test]
    fn l16_round_trip_stays_within_one_lsb() {
        // The main-tree converter truncates (round-to-nearest + dither is
        // the L16 work landing separately): one LSB of error is the bound.
        let mut decoded = Vec::new();
        l16_le_to_f32_into(&l16(&[0.0, 0.5, -0.5]), &mut decoded);
        for (expected, actual) in [0.0, 0.5, -0.5].iter().zip(&decoded) {
            assert!(
                (expected - actual).abs() <= 1.0 / i16::MAX as f32,
                "round trip of {expected} came back as {actual}"
            );
        }
    }
}
