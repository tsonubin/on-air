use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const CHUNK_SIZE: usize = 1024;

pub struct MonoResampler {
    inner: Async<f32>,
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
        MonoResampler { inner }
    }

    pub fn input_frames_next(&self) -> usize {
        self.inner.input_frames_next()
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        assert_eq!(
            input.len(),
            self.inner.input_frames_next(),
            "MonoResampler::process requires exactly input_frames_next() samples"
        );
        let input_data = vec![input.to_vec()];
        let out_capacity = self.inner.output_frames_max();
        let mut output_data = vec![vec![0.0f32; out_capacity]];

        let in_adapter = SequentialSliceOfVecs::new(&input_data, 1, input.len())
            .expect("valid input adapter shape");
        let mut out_adapter = SequentialSliceOfVecs::new_mut(&mut output_data, 1, out_capacity)
            .expect("valid output adapter shape");

        let (_frames_read, frames_written) = self
            .inner
            .process_into_buffer(&in_adapter, &mut out_adapter, None)
            .expect("resample succeeds for a full, correctly-sized chunk");

        output_data.remove(0).into_iter().take(frames_written).collect()
    }
}
