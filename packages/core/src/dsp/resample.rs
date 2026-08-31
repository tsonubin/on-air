use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const CHUNK_SIZE: usize = 1024;

pub struct MonoResampler {
    inner: Async<f32>,
    input_data: Vec<Vec<f32>>,
    output_data: Vec<Vec<f32>>,
}

impl MonoResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32) -> Self {
        let ratio = output_rate_hz as f64 / input_rate_hz as f64;
        let inner = Async::<f32>::new_poly(
            ratio,
            1.1,
            PolynomialDegree::Cubic,
            CHUNK_SIZE,
            1,
            FixedAsync::Input,
        )
        .expect("valid resample ratio");
        let input_data = vec![vec![0.0; inner.input_frames_next()]];
        let output_data = vec![vec![0.0; inner.output_frames_max()]];
        MonoResampler {
            inner,
            input_data,
            output_data,
        }
    }

    pub fn input_frames_next(&self) -> usize {
        self.inner.input_frames_next()
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(self.inner.output_frames_max());
        self.process_into(input, &mut output);
        output
    }

    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        assert_eq!(
            input.len(),
            self.inner.input_frames_next(),
            "MonoResampler::process requires exactly input_frames_next() samples"
        );
        self.input_data[0].copy_from_slice(input);
        let out_capacity = self.inner.output_frames_max();

        let in_adapter = SequentialSliceOfVecs::new(&self.input_data, 1, input.len())
            .expect("valid input adapter shape");
        let mut out_adapter =
            SequentialSliceOfVecs::new_mut(&mut self.output_data, 1, out_capacity)
                .expect("valid output adapter shape");

        let (_frames_read, frames_written) = self
            .inner
            .process_into_buffer(&in_adapter, &mut out_adapter, None)
            .expect("resample succeeds for a full, correctly-sized chunk");

        output.clear();
        output.extend_from_slice(&self.output_data[0][..frames_written]);
    }
}

/// Buffers arbitrary-sized mono chunks into `MonoResampler`'s fixed input size.
/// Same-rate I/O is a lossless passthrough (no rubato delay).
pub struct StreamingMonoResampler {
    input_rate_hz: u32,
    output_rate_hz: u32,
    inner: Option<MonoResampler>,
    pending: Vec<f32>,
}

impl StreamingMonoResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32) -> Self {
        let inner = if input_rate_hz == output_rate_hz {
            None
        } else {
            Some(MonoResampler::new(input_rate_hz, output_rate_hz))
        };
        StreamingMonoResampler {
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
            let need = resampler.input_frames_next();
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
