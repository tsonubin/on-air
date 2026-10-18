use super::resample::{RateControl, SteerableResampler, StreamingResampler};
use super::PIPELINE_CHANNELS;

/// Decode pipeline stereo L16 LE, resample to the output rate, then fit it to
/// the destination channel count. This is the sender-side half of the
/// input/output sample-rate bridge.
///
/// [`RateBridge::new`] is the cheap fixed-ratio path used by free-running
/// outputs. [`RateBridge::new_steered`] builds the drift-corrected path: a
/// variable-ratio resampler steered through the returned [`RateControl`],
/// used by sync-group members.
pub struct RateBridge {
    core: RateCore,
    channels: usize,
}

enum RateCore {
    Fixed(StreamingResampler),
    Steered {
        resampler: SteerableResampler,
        control: RateControl,
        applied_ppm: f64,
    },
}

impl RateBridge {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        RateBridge {
            core: RateCore::Fixed(StreamingResampler::new(
                input_rate_hz,
                output_rate_hz,
                PIPELINE_CHANNELS,
            )),
            channels: channels.max(1),
        }
    }

    /// The drift-corrected bridge for a sync-group member, plus the handle
    /// its drift observer steers the ratio with.
    pub fn new_steered(
        input_rate_hz: u32,
        output_rate_hz: u32,
        channels: usize,
    ) -> (Self, RateControl) {
        let control = RateControl::default();
        let bridge = RateBridge {
            core: RateCore::Steered {
                resampler: SteerableResampler::new(
                    input_rate_hz,
                    output_rate_hz,
                    PIPELINE_CHANNELS,
                ),
                control: control.clone(),
                applied_ppm: 0.0,
            },
            channels: channels.max(1),
        };
        (bridge, control)
    }

    pub fn process_l16(&mut self, pcm: &[u8]) -> Vec<f32> {
        let samples = l16_le_to_f32(pcm);
        let resampled = match &mut self.core {
            RateCore::Fixed(resampler) => resampler.process(&samples),
            RateCore::Steered {
                resampler,
                control,
                applied_ppm,
            } => {
                // One relaxed atomic read per chunk, and a live ratio update
                // only when the drift observer actually moved the correction.
                let want = control.correction_ppm();
                if want != *applied_ppm {
                    resampler.set_rate_correction_ppm(want);
                    *applied_ppm = want;
                }
                resampler.process(&samples)
            }
        };
        map_stereo(&resampled, self.channels)
    }

    pub fn process_l16_to_l16(&mut self, pcm: &[u8]) -> Vec<u8> {
        f32_to_l16_le(&self.process_l16(pcm))
    }

    pub fn output_rate_hz(&self) -> u32 {
        match &self.core {
            RateCore::Fixed(resampler) => resampler.output_rate_hz(),
            RateCore::Steered { resampler, .. } => resampler.output_rate_hz(),
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }
}

pub fn l16_le_to_f32(pcm: &[u8]) -> Vec<f32> {
    let (samples, _) = pcm.as_chunks::<2>();
    samples
        .iter()
        .map(|sample| i16::from_le_bytes(*sample) as f32 / i16::MAX as f32)
        .collect()
}

pub fn f32_to_l16_le(samples: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let sample = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }
    buf
}

/// Fits interleaved pipeline stereo to a destination with `channels`
/// channels: one channel gets the average of left and right, two pass
/// through, and any further channels get silence.
pub fn map_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    if channels == PIPELINE_CHANNELS {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len() / PIPELINE_CHANNELS * channels);
    let (frames, _) = samples.as_chunks::<PIPELINE_CHANNELS>();
    for frame in frames {
        if channels == 1 {
            out.push((frame[0] + frame[1]) * 0.5);
        } else {
            out.extend_from_slice(frame);
            out.extend(std::iter::repeat_n(0.0, channels - PIPELINE_CHANNELS));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_stereo_fits_the_destination_layout() {
        assert_eq!(map_stereo(&[0.5, 0.25], 1), vec![0.375]);
        assert_eq!(map_stereo(&[0.5, 0.25], 2), vec![0.5, 0.25]);
        assert_eq!(map_stereo(&[0.5, 0.25], 4), vec![0.5, 0.25, 0.0, 0.0]);
    }

    #[test]
    fn same_rate_bridge_keeps_left_and_right() {
        let mut bridge = RateBridge::new(44_100, 44_100, 2);
        let out = bridge.process_l16(&f32_to_l16_le(&[0.25, -0.25]));
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.25).abs() < 0.01 && (out[1] + 0.25).abs() < 0.01);
    }

    #[test]
    fn mono_destination_hears_both_sides() {
        let mut bridge = RateBridge::new(44_100, 44_100, 1);
        let out = bridge.process_l16(&f32_to_l16_le(&[0.5, 0.0]));
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.25).abs() < 0.01);
    }
}
