use super::resample::StreamingMonoResampler;

/// Decode pipeline L16 LE mono, resample to the output rate, then upmix to
/// the destination channel count. This is the sender-side half of the
/// input/output sample-rate bridge.
pub struct RateBridge {
    resampler: StreamingMonoResampler,
    channels: usize,
}

impl RateBridge {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        RateBridge {
            resampler: StreamingMonoResampler::new(input_rate_hz, output_rate_hz),
            channels: channels.max(1),
        }
    }

    pub fn process_l16_mono(&mut self, pcm: &[u8]) -> Vec<f32> {
        let mono = l16_le_to_f32(pcm);
        let resampled = self.resampler.process(&mono);
        upmix_mono(&resampled, self.channels)
    }

    pub fn process_l16_mono_to_l16(&mut self, pcm: &[u8]) -> Vec<u8> {
        f32_to_l16_le(&self.process_l16_mono(pcm))
    }

    pub fn output_rate_hz(&self) -> u32 {
        self.resampler.output_rate_hz()
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

pub fn upmix_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len() * channels);
    for &sample in samples {
        for _ in 0..channels {
            out.push(sample);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upmix_duplicates_each_mono_sample() {
        assert_eq!(upmix_mono(&[0.1, -0.2], 2), vec![0.1, 0.1, -0.2, -0.2]);
        assert_eq!(upmix_mono(&[0.5], 1), vec![0.5]);
    }

    #[test]
    fn same_rate_stereo_bridge_does_not_change_pitch() {
        let mut bridge = RateBridge::new(44_100, 44_100, 2);
        let pcm = f32_to_l16_le(&[0.25, -0.25]);
        let out = bridge.process_l16_mono(&pcm);
        assert_eq!(out.len(), 4);
        assert!((out[0] - out[1]).abs() < f32::EPSILON);
        assert!((out[0] - 0.25).abs() < 0.01);
    }
}
