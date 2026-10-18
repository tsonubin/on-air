use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Adjustable, Async, FixedAsync, PolynomialDegree, Resampler, SincInterpolationParameters,
    WindowFunction,
};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

const CHUNK_SIZE: usize = 1024;

/// The drift-corrected path never steers further than this from the nominal
/// ratio (±2%); a consumer outside that band is stalled or bursting, not
/// drifting. Also the bound rubato enforces at construction.
const MAX_CORRECTION_PPM: f64 = 20_000.0;

/// Steering handle shared between a drift observer and the resampler it
/// steers. Steered rate stages read it once per chunk (one relaxed atomic
/// load); fixed rate stages never touch it.
#[derive(Clone, Debug, Default)]
pub struct RateControl {
    correction_ppm_milli: Arc<AtomicI64>,
}

impl RateControl {
    /// The drift correction to apply, in parts per million of the nominal
    /// ratio. Clamped to ±2%.
    pub fn set_correction_ppm(&self, ppm: f64) {
        let clamped = ppm.clamp(-MAX_CORRECTION_PPM, MAX_CORRECTION_PPM);
        self.correction_ppm_milli
            .store((clamped * 1000.0).round() as i64, Ordering::Relaxed);
    }

    pub fn correction_ppm(&self) -> f64 {
        self.correction_ppm_milli.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

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

/// Variable-ratio resampler for the drift-corrected (sync group) path.
///
/// rubato's async SINC (`Async::<f32>::new_sinc`) is both higher quality
/// than the cheap cubic Farrow used by [`PcmResampler`] and steerable live
/// through [`Adjustable::set_resample_ratio`], which is what drift
/// correction needs: a consumer whose clock runs fast must be fed a
/// proportionally stretched stream. The non-synchronized path keeps the
/// fixed-ratio resamplers above.
///
/// Cost per chunk: a windowed sinc interpolation (sinc length 128, 128×
/// oversampled filter table, cubic interpolation between table points) plus
/// one ratio update only when the correction changed. This runs only for
/// sync-group members, never on the free-running path.
pub struct SteerableResampler {
    inner: Async<f32>,
    channels: usize,
    input_rate_hz: u32,
    output_rate_hz: u32,
    nominal_ratio: f64,
    ratio: f64,
    output_data: Vec<f32>,
    pending: Vec<f32>,
}

impl SteerableResampler {
    pub fn new(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        let ratio = f64::from(output_rate_hz) / f64::from(input_rate_hz);
        let parameters = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2);
        let inner = Async::<f32>::new_sinc(
            ratio,
            1.0 + MAX_CORRECTION_PPM / 1e6,
            &parameters,
            CHUNK_SIZE,
            channels,
            FixedAsync::Input,
        )
        .expect("valid resample ratio");
        let output_data = vec![0.0; inner.output_frames_max() * channels];
        SteerableResampler {
            inner,
            channels,
            input_rate_hz,
            output_rate_hz,
            nominal_ratio: ratio,
            ratio,
            output_data,
            pending: Vec::new(),
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn input_rate_hz(&self) -> u32 {
        self.input_rate_hz
    }

    pub fn output_rate_hz(&self) -> u32 {
        self.output_rate_hz
    }

    /// The ratio currently applied (output frames per input frame).
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Steer the ratio by `correction_ppm` parts per million of nominal.
    /// Clamped to ±2% like [`RateControl`]; the clamp is a guard against
    /// feeding a stalled consumer's burst into the resampler, not a claim
    /// that real drift ever needs more.
    pub fn set_rate_correction_ppm(&mut self, correction_ppm: f64) {
        let clamped = correction_ppm.clamp(-MAX_CORRECTION_PPM, MAX_CORRECTION_PPM);
        let ratio = self.nominal_ratio * (1.0 + clamped / 1e6);
        if ratio == self.ratio {
            return;
        }
        self.inner
            .set_resample_ratio(ratio, true)
            .expect("correction stays inside the constructed ratio bounds");
        self.ratio = ratio;
    }

    /// Streaming conversion: buffers arbitrary whole-frame chunks into the
    /// resampler's fixed input size. One reusable output buffer, no
    /// allocation per call.
    pub fn process_into(&mut self, input: &[f32], output: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        output.clear();
        let mut consumed = 0;
        loop {
            let frames = self.inner.input_frames_next();
            let need = frames * self.channels;
            if self.pending.len() - consumed < need {
                break;
            }
            let in_adapter =
                InterleavedSlice::new(&self.pending[consumed..consumed + need], self.channels, frames)
                    .expect("valid input adapter shape");
            let out_capacity = self.inner.output_frames_max();
            let mut out_adapter =
                InterleavedSlice::new_mut(&mut self.output_data, self.channels, out_capacity)
                    .expect("valid output adapter shape");
            let (_frames_read, frames_written) = self
                .inner
                .process_into_buffer(&in_adapter, &mut out_adapter, None)
                .expect("resample succeeds for a full, correctly-sized chunk");
            output.extend_from_slice(&self.output_data[..frames_written * self.channels]);
            consumed += need;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::new();
        self.process_into(input, &mut output);
        output
    }
}
