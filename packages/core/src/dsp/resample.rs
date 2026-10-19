use crate::dsp::DspQuality;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, PolynomialDegree, Resampler, SincInterpolationParameters, WindowFunction,
};

const CHUNK_SIZE: usize = 1024;

/// Sinc length of the `balanced` profile. Measured cost per 1024-frame
/// stereo chunk (44.1→48 kHz, release, dev host) is roughly 20× the cubic
/// Farrow below — an order of magnitude, not a rounding error, which is
/// why the audio check measures the ladder on the machine that will run
/// it instead of trusting these numbers.
const BALANCED_SINC_LEN: usize = 128;
/// Sinc length of the `high` profile: about 1.5× `balanced` per chunk on
/// the same run, for a filter table twice the size (worse cache behaviour
/// on old hardware).
const HIGH_SINC_LEN: usize = 256;

/// Build the rubato async resampler for one quality profile at a fixed
/// input chunk size. [`DspQuality::Economy`] is the cubic Farrow polynomial
/// interpolator — byte-for-byte the resampler every path used before
/// quality profiles existed. [`DspQuality::Balanced`] and
/// [`DspQuality::High`] are windowed sinc interpolation.
fn build_async(
    ratio: f64,
    max_relative_ratio: f64,
    channels: usize,
    quality: DspQuality,
) -> Async<f32> {
    match quality {
        DspQuality::Economy => Async::<f32>::new_poly(
            ratio,
            max_relative_ratio,
            PolynomialDegree::Cubic,
            CHUNK_SIZE,
            channels,
            FixedAsync::Input,
        ),
        DspQuality::Balanced | DspQuality::High => {
            let sinc_len = match quality {
                DspQuality::High => HIGH_SINC_LEN,
                _ => BALANCED_SINC_LEN,
            };
            let parameters =
                SincInterpolationParameters::new(sinc_len, WindowFunction::BlackmanHarris2);
            Async::<f32>::new_sinc(
                ratio,
                max_relative_ratio,
                &parameters,
                CHUNK_SIZE,
                channels,
                FixedAsync::Input,
            )
        }
    }
    .expect("valid resample ratio")
}

/// Resamples fixed-size chunks of interleaved PCM with `channels` samples
/// per frame. `quality` selects the interpolation profile at construction.
///
/// [`PcmResampler::process_into`] allocates nothing per call in steady
/// state (the C7 allocation guard proves it): the output buffer is owned
/// by the caller and the internal scratch by the resampler.
pub struct PcmResampler {
    inner: Async<f32>,
    channels: usize,
    output_data: Vec<f32>,
}

impl PcmResampler {
    pub fn new(
        input_rate_hz: u32,
        output_rate_hz: u32,
        channels: usize,
        quality: DspQuality,
    ) -> Self {
        let channels = channels.max(1);
        let ratio = output_rate_hz as f64 / input_rate_hz as f64;
        let inner = build_async(ratio, 1.1, channels, quality);
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

    /// Frames (not samples) the next `process_into` call needs.
    pub fn input_frames_next(&self) -> usize {
        self.inner.input_frames_next()
    }

    /// Resamples exactly `input_frames_next() * channels` samples into
    /// `output`, which is cleared and filled with this call's output.
    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let frames = self.inner.input_frames_next();
        assert_eq!(
            input.len(),
            frames * self.channels,
            "PcmResampler::process_into requires exactly input_frames_next() * channels samples"
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
///
/// [`StreamingResampler::process_into`] allocates nothing per call in
/// steady state when `output` is reused across calls (the C7 allocation
/// guard proves it).
pub struct StreamingResampler {
    input_rate_hz: u32,
    output_rate_hz: u32,
    inner: Option<PcmResampler>,
    pending: Vec<f32>,
    /// One fixed chunk of resampled output, reused across calls.
    scratch: Vec<f32>,
}

impl StreamingResampler {
    pub fn new(
        input_rate_hz: u32,
        output_rate_hz: u32,
        channels: usize,
        quality: DspQuality,
    ) -> Self {
        let inner = if input_rate_hz == output_rate_hz {
            None
        } else {
            Some(PcmResampler::new(
                input_rate_hz,
                output_rate_hz,
                channels,
                quality,
            ))
        };
        StreamingResampler {
            input_rate_hz,
            output_rate_hz,
            inner,
            pending: Vec::new(),
            scratch: Vec::new(),
        }
    }

    pub fn input_rate_hz(&self) -> u32 {
        self.input_rate_hz
    }

    pub fn output_rate_hz(&self) -> u32 {
        self.output_rate_hz
    }

    /// Resamples `input` into `output`, which is cleared and filled with
    /// the PCM this call produced (empty until enough input has arrived to
    /// fill a fixed chunk). `output` must not alias `input`.
    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let Some(resampler) = self.inner.as_mut() else {
            output.clear();
            output.extend_from_slice(input);
            return;
        };
        self.pending.extend_from_slice(input);
        output.clear();
        let mut consumed = 0;
        loop {
            let need = resampler.input_frames_next() * resampler.channels();
            if self.pending.len() - consumed < need {
                break;
            }
            resampler.process_into(&self.pending[consumed..consumed + need], &mut self.scratch);
            output.extend_from_slice(&self.scratch);
            consumed += need;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
    }
}
