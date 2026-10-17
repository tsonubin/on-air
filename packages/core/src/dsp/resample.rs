use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const CHUNK_SIZE: usize = 1024;

/// Resamples fixed-size chunks of interleaved PCM with `channels` samples
/// per frame.
pub struct PcmResampler {
    inner: Async<f32>,
    channels: usize,
    output_data: Vec<f32>,
}

impl PcmResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        let ratio = output_rate_hz as f64 / input_rate_hz as f64;
        let inner = Async::<f32>::new_poly(
            ratio,
            1.1,
            PolynomialDegree::Cubic,
            CHUNK_SIZE,
            channels,
            FixedAsync::Input,
        )
        .expect("valid resample ratio");
        let output_data = vec![0.0; inner.output_frames_max() * channels];
        PcmResampler {
            inner,
            channels,
            output_data,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Frames (not samples) the next `process` call needs.
    pub fn input_frames_next(&self) -> usize {
        self.inner.input_frames_next()
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(self.output_data.len());
        self.process_into(input, &mut output);
        output
    }

    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let frames = self.inner.input_frames_next();
        assert_eq!(
            input.len(),
            frames * self.channels,
            "PcmResampler::process requires exactly input_frames_next() * channels samples"
        );
        let out_capacity = self.inner.output_frames_max();

        let in_adapter =
            InterleavedSlice::new(input, self.channels, frames).expect("valid input adapter shape");
        let mut out_adapter =
            InterleavedSlice::new_mut(&mut self.output_data, self.channels, out_capacity)
                .expect("valid output adapter shape");

        let (_frames_read, frames_written) = self
            .inner
            .process_into_buffer(&in_adapter, &mut out_adapter, None)
            .expect("resample succeeds for a full, correctly-sized chunk");

        output.clear();
        output.extend_from_slice(&self.output_data[..frames_written * self.channels]);
    }
}

/// Buffers arbitrary whole-frame chunks into `PcmResampler`'s fixed input
/// size. Same-rate I/O is a lossless passthrough (no rubato delay).
pub struct StreamingResampler {
    input_rate_hz: u32,
    output_rate_hz: u32,
    inner: Option<PcmResampler>,
    pending: Vec<f32>,
}

impl StreamingResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        let inner = if input_rate_hz == output_rate_hz {
            None
        } else {
            Some(PcmResampler::new(input_rate_hz, output_rate_hz, channels))
        };
        StreamingResampler {
            input_rate_hz,
            output_rate_hz,
            inner,
            pending: Vec::new(),
        }
    }

    pub fn input_rate_hz(&self) -> u32 {
        self.input_rate_hz
    }

    pub fn output_rate_hz(&self) -> u32 {
        self.output_rate_hz
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let Some(resampler) = self.inner.as_mut() else {
            return input.to_vec();
        };
        self.pending.extend_from_slice(input);
        let mut output = Vec::new();
        let mut resampled = Vec::new();
        let mut consumed = 0;
        loop {
            let need = resampler.input_frames_next() * resampler.channels();
            if self.pending.len() - consumed < need {
                break;
            }
            resampler.process_into(&self.pending[consumed..consumed + need], &mut resampled);
            output.extend_from_slice(&resampled);
            consumed += need;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        output
    }
}
