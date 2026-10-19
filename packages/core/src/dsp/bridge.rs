use super::channel::ChannelPlan;
use super::resample::{RateControl, SteerableResampler, StreamingResampler};
use super::{DspQuality, PIPELINE_CHANNELS};

/// Decode pipeline stereo L16 LE, resample to the output rate, then fit it to
/// the destination channel count. This is the sender-side half of the
/// input/output sample-rate bridge.
///
/// [`RateBridge::new`] is the cheap fixed-ratio path used by free-running
/// outputs. [`RateBridge::new_steered`] builds the drift-corrected path: a
/// variable-ratio resampler steered through the returned [`RateControl`],
/// used by sync-group members.
///
/// `plan` justifies the channel fit: a stereo destination keeps both
/// channels, and a request of 1 happens only for a genuinely mono
/// destination or the user's explicit mono mode (roadmap A4/A6; see
/// [`super::channel`]). `quality` selects the resampler interpolation
/// profile at construction. The `*_into` conversions keep their scratch
/// inside the bridge and hand output to a caller-owned buffer: they allocate
/// nothing per chunk in steady state (the C7 allocation guard proves it), so
/// callers must reuse their output buffer across chunks.
pub struct RateBridge {
    core: RateCore,
    channels: usize,
    dither: TpdfDither,
    /// Scratch: decoded f32 input.
    decoded: Vec<f32>,
    /// Scratch: resampled stereo.
    resampled: Vec<f32>,
    /// Scratch: channel-mapped output for the L16 encode path.
    mapped: Vec<f32>,
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
    /// The cheap fixed-ratio path used by free-running outputs. `quality`
    /// selects the interpolation profile of the rate conversion.
    pub fn new(
        input_rate_hz: u32,
        output_rate_hz: u32,
        plan: &ChannelPlan,
        quality: DspQuality,
    ) -> Self {
        RateBridge {
            core: RateCore::Fixed(StreamingResampler::new(
                input_rate_hz,
                output_rate_hz,
                &plan.pipeline_side(),
                quality,
            )),
            channels: plan.channels().max(1),
            dither: TpdfDither::for_stream(input_rate_hz, output_rate_hz, plan.channels()),
            decoded: Vec::new(),
            resampled: Vec::new(),
            mapped: Vec::new(),
        }
    }

    /// The drift-corrected bridge for a sync-group member, plus the handle
    /// its drift observer steers the ratio with.
    pub fn new_steered(
        input_rate_hz: u32,
        output_rate_hz: u32,
        plan: &ChannelPlan,
        quality: DspQuality,
    ) -> (Self, RateControl) {
        let control = RateControl::default();
        let bridge = RateBridge {
            core: RateCore::Steered {
                resampler: SteerableResampler::new(
                    input_rate_hz,
                    output_rate_hz,
                    &plan.pipeline_side(),
                    quality,
                ),
                control: control.clone(),
                applied_ppm: 0.0,
            },
            channels: plan.channels().max(1),
            dither: TpdfDither::for_stream(input_rate_hz, output_rate_hz, plan.channels()),
            decoded: Vec::new(),
            resampled: Vec::new(),
            mapped: Vec::new(),
        };
        (bridge, control)
    }

    /// Decodes, resamples and channel-maps `pcm` into `out`, which is
    /// cleared and filled with this call's PCM.
    pub fn process_l16_into(&mut self, pcm: &[u8], out: &mut Vec<f32>) {
        let Self {
            core,
            channels,
            decoded,
            resampled,
            ..
        } = self;
        l16_le_to_f32_into(pcm, decoded);
        run_core(core, decoded, resampled);
        map_stereo_into(resampled, *channels, out);
    }

    /// Like [`RateBridge::process_l16_into`], ending at L16 LE for the wire.
    pub fn process_l16_to_l16_into(&mut self, pcm: &[u8], out: &mut Vec<u8>) {
        let Self {
            core,
            channels,
            decoded,
            resampled,
            mapped,
            dither,
        } = self;
        l16_le_to_f32_into(pcm, decoded);
        run_core(core, decoded, resampled);
        map_stereo_into(resampled, *channels, mapped);
        f32_to_l16_le_into(mapped, dither, out);
    }

    /// The allocating form of [`RateBridge::process_l16_into`].
    pub fn process_l16(&mut self, pcm: &[u8]) -> Vec<f32> {
        let mut out = Vec::new();
        self.process_l16_into(pcm, &mut out);
        out
    }

    /// The allocating form of [`RateBridge::process_l16_to_l16_into`].
    pub fn process_l16_to_l16(&mut self, pcm: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        self.process_l16_to_l16_into(pcm, &mut out);
        out
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

/// One chunk through the rate core: fixed-ratio resampling, or the drift-
/// steered path with one relaxed [`RateControl`] read per chunk and a live
/// ratio update only when the drift observer actually moved the correction.
fn run_core(core: &mut RateCore, decoded: &[f32], resampled: &mut Vec<f32>) {
    match core {
        RateCore::Fixed(resampler) => resampler.process_into(decoded, resampled),
        RateCore::Steered {
            resampler,
            control,
            applied_ppm,
        } => {
            let want = control.correction_ppm();
            if want != *applied_ppm {
                resampler.set_rate_correction_ppm(want);
                *applied_ppm = want;
            }
            resampler.process_into(decoded, resampled);
        }
    }
}

/// Full-scale magnitude for L16. Using the power of two rather than
/// `i16::MAX` keeps encode and decode exact inverses, so a float round trip
/// through L16 has unity gain and no DC bias.
const L16_SCALE: f32 = 32768.0;

/// Interleaved stereo L16 LE → interleaved f32, appended to `out` after it
/// is cleared. A trailing partial frame is dropped.
pub fn l16_le_to_f32_into(pcm: &[u8], out: &mut Vec<f32>) {
    let (samples, _) = pcm.as_chunks::<2>();
    out.clear();
    out.reserve(samples.len());
    out.extend(
        samples
            .iter()
            .map(|sample| i16::from_le_bytes(*sample) as f32 / L16_SCALE),
    );
}

/// The allocating form of [`l16_le_to_f32_into`].
pub fn l16_le_to_f32(pcm: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();
    l16_le_to_f32_into(pcm, &mut out);
    out
}

/// Triangular-PDF dither for float to L16 quantization.
///
/// Quantizing to 16 bits rounds each sample to a code; rounding alone leaves
/// an error that is correlated with the signal and is heard as distortion and
/// noise modulation on quiet passages. Adding one LSB peak-to-peak of
/// triangular noise before rounding decorrelates that error, so what is left
/// is a constant, signal-independent hiss floor at about -93 dBFS.
///
/// The triangular PDF is the sum of two independent uniform generators, each
/// spanning half an LSB. One generator would leave a *rectangular* PDF, which
/// still correlates with the signal.
///
/// Each stream owns its own instance seeded differently, so the dither of two
/// senders never lines up and sounds doubled.
pub struct TpdfDither {
    state: u64,
}

impl TpdfDither {
    /// `seed` selects the noise sequence and must not be zero. Streams pick
    /// different seeds; see [`TpdfDither::for_stream`].
    pub const fn new(seed: u64) -> Self {
        TpdfDither {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// A deterministic but distinct seed per stream, derived from the stream's
    /// sample rates and channel count.
    pub const fn for_stream(input_rate_hz: u32, output_rate_hz: u32, channels: usize) -> Self {
        Self::new(
            (input_rate_hz as u64) << 32
                ^ (output_rate_hz as u64) << 8
                ^ channels as u64
                ^ 0xC0FF_EE00_D15E_A5E5,
        )
    }

    /// xorshift64*: one non-zero state word, no allocation, no branch. This
    /// runs per sample on hardware that may be a decade old, so it stays
    /// cheaper than the multiplication it decorrelates.
    #[inline]
    fn uniform(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        let bits = x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40;
        // 24 bits is well past the 16-bit target and keeps the mapping exact.
        (bits as f32 / (1u64 << 24) as f32) - 0.5
    }

    /// One dither sample in LSBs, spanning (-1, 1) with a triangular PDF.
    #[inline]
    pub fn sample(&mut self) -> f32 {
        self.uniform() + self.uniform()
    }
}

/// Quantize interleaved float samples to L16 LE, rounding to nearest and
/// adding one LSB of [`TpdfDither`] first. Written to `out`, which is
/// cleared first.
///
/// `dither` is per stream so callers that interleave several senders keep
/// their noise uncorrelated. Clamping happens before dither so a full-scale
/// sample cannot be pushed past the i16 range and fold over.
pub fn f32_to_l16_le_into(samples: &[f32], dither: &mut TpdfDither, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * 2);
    for &s in samples {
        let scaled = s.clamp(-1.0, 1.0) * L16_SCALE + dither.sample();
        // Round half away from zero, then saturate. `as i16` saturates in
        // Rust, which is what we want at the rails.
        let sample = scaled.round() as i16;
        out.extend_from_slice(&sample.to_le_bytes());
    }
}

/// The allocating form of [`f32_to_l16_le_into`].
pub fn f32_to_l16_le(samples: &[f32], dither: &mut TpdfDither) -> Vec<u8> {
    let mut buf = Vec::with_capacity(samples.len() * 2);
    f32_to_l16_le_into(samples, dither, &mut buf);
    buf
}

/// Fits interleaved pipeline stereo to a destination with `channels`
/// channels: one channel gets the average of left and right (a true
/// downmix — the stream becomes genuinely mono), two pass through, and any
/// further channels get silence. Silence on channels 3+ is the explicit,
/// tested policy for wide destinations: the pipeline has no content for
/// them (roadmap A4/A6; `tests/channel_construction.rs` pins it). Written
/// to `out`, which is cleared first and must not alias `samples`.
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

/// The allocating form of [`map_stereo_into`].
pub fn map_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    let mut out = Vec::new();
    map_stereo_into(samples, channels, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::channel::{ChannelMode, Destination, SinkConstruction};

    /// One quantization step, in float units.
    const LSB: f32 = 1.0 / L16_SCALE;

    fn plan_for(destination: Destination, mode: ChannelMode) -> ChannelPlan {
        SinkConstruction::CpalPcm
            .plan(mode, destination)
            .expect("stereo and mono destinations fit every mode but stereo-on-mono")
    }

    #[test]
    fn map_stereo_fits_the_destination_layout() {
        let mut out = Vec::new();
        map_stereo_into(&[0.5, 0.25], 1, &mut out);
        assert_eq!(out, vec![0.375]);
        map_stereo_into(&[0.5, 0.25], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.25]);
        map_stereo_into(&[0.5, 0.25], 4, &mut out);
        assert_eq!(out, vec![0.5, 0.25, 0.0, 0.0]);
        // The allocating form agrees.
        assert_eq!(map_stereo(&[0.5, 0.25], 1), vec![0.375]);
        assert_eq!(map_stereo(&[0.5, 0.25], 2), vec![0.5, 0.25]);
        assert_eq!(map_stereo(&[0.5, 0.25], 4), vec![0.5, 0.25, 0.0, 0.0]);
    }

    fn l16(samples: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        f32_to_l16_le_into(samples, &mut TpdfDither::new(1), &mut out);
        out
    }

    #[test]
    fn same_rate_bridge_keeps_left_and_right() {
        let plan = plan_for(Destination::stereo("Speakers"), ChannelMode::Auto);
        let mut dither = TpdfDither::new(1);
        let mut bridge = RateBridge::new(44_100, 44_100, &plan, DspQuality::Economy);
        let out = bridge.process_l16(&f32_to_l16_le(&[0.25, -0.25], &mut dither));
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.25).abs() < 0.01 && (out[1] + 0.25).abs() < 0.01);

        // The in-place path agrees.
        let mut bridge = RateBridge::new(44_100, 44_100, &plan, DspQuality::Economy);
        let mut out = Vec::new();
        bridge.process_l16_into(&l16(&[0.25, -0.25]), &mut out);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.25).abs() < 0.01 && (out[1] + 0.25).abs() < 0.01);
    }

    #[test]
    fn mono_destination_hears_both_sides() {
        let plan = plan_for(Destination::mono("Mono Speaker"), ChannelMode::Auto);
        let mut dither = TpdfDither::new(1);
        let mut bridge = RateBridge::new(44_100, 44_100, &plan, DspQuality::Economy);
        let out = bridge.process_l16(&f32_to_l16_le(&[0.5, 0.0], &mut dither));
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.25).abs() < 0.01);

        // The in-place path agrees.
        let mut bridge = RateBridge::new(44_100, 44_100, &plan, DspQuality::Economy);
        let mut out = Vec::new();
        bridge.process_l16_into(&l16(&[0.5, 0.0]), &mut out);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.25).abs() < 0.01);
    }

    #[test]
    fn mono_mode_downmixes_a_stereo_destination_to_a_genuine_mono_stream() {
        let plan = plan_for(Destination::stereo("Speakers"), ChannelMode::Mono);
        assert_eq!(plan.channels(), 1);
        let mut dither = TpdfDither::new(1);
        let mut bridge = RateBridge::new(44_100, 44_100, &plan, DspQuality::Economy);
        let out = bridge.process_l16(&f32_to_l16_le(&[0.5, -0.5, 0.25, 0.75], &mut dither));
        // One sample per frame — a true downmix, not a duplicated mono pair.
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.0).abs() < 0.01 && (out[1] - 0.5).abs() < 0.01);
    }

    #[test]
    fn l16_round_trip_stays_within_one_lsb() {
        // Round-to-nearest with one LSB of TPDF dither: one LSB of error is
        // the bound.
        let mut decoded = Vec::new();
        l16_le_to_f32_into(&l16(&[0.0, 0.5, -0.5]), &mut decoded);
        for (expected, actual) in [0.0, 0.5, -0.5].iter().zip(&decoded) {
            assert!(
                (expected - actual).abs() <= 1.0 / i16::MAX as f32,
                "round trip of {expected} came back as {actual}"
            );
        }
    }

    /// The test that pins the dither. A signal sitting at exactly half a code
    /// is where a truncating converter is worst: it always rounds the same
    /// way, so the error is a fixed offset rather than noise. Dithered
    /// rounding spreads that error evenly around zero.
    ///
    /// This is audible, not academic: a fixed error is harmonic distortion on
    /// quiet material, and it modulates with the signal.
    #[test]
    fn quantization_error_is_unbiased() {
        let mut dither = TpdfDither::new(0x5EED_1234_ABCD_0007);
        const N: usize = 16_384;
        let wanted = vec![0.5 * LSB; N];
        let decoded = l16_le_to_f32(&f32_to_l16_le(&wanted, &mut dither));
        let mean_error = decoded.iter().zip(&wanted).map(|(d, s)| d - s).sum::<f32>() / N as f32;
        // A truncating converter lands on -0.5 LSB here, which is 2.5x this
        // bound. The dithered mean sits within a few thousandths of an LSB.
        assert!(
            mean_error.abs() < 0.2 * LSB,
            "quantization error is biased by {mean_error} ({} LSB)",
            mean_error / LSB
        );
    }

    #[test]
    fn out_of_range_input_saturates_rather_than_folding_over() {
        let mut dither = TpdfDither::new(3);
        let bytes = f32_to_l16_le(&[8.0, -8.0], &mut dither);
        let hi = i16::from_le_bytes(bytes[0..2].try_into().unwrap());
        let lo = i16::from_le_bytes(bytes[2..4].try_into().unwrap());
        assert!(hi > 32_000 && lo < -32_000, "rails folded over: {hi} {lo}");
    }

    #[test]
    fn dither_stays_within_one_lsb_and_centres_on_zero() {
        let mut dither = TpdfDither::new(0xDEAD_BEEF_CAFE_1234);
        const N: usize = 16_384;
        let mut sum = 0.0f64;
        let mut peak = 0.0f32;
        for _ in 0..N {
            let sample = dither.sample();
            assert!(sample.is_finite(), "dither produced a non-finite sample");
            sum += f64::from(sample);
            peak = peak.max(sample.abs());
        }
        assert!(peak <= 1.0, "dither exceeded one LSB: {peak}");
        let mean = sum / N as f64;
        assert!(mean.abs() < 0.01, "dither is biased by {mean}");
    }

    /// Two streams must not carry the same noise, or mixing them would
    /// correlate the dither and undo its point.
    #[test]
    fn separate_streams_take_separate_dither() {
        let mut a = TpdfDither::for_stream(44_100, 48_000, 2);
        let mut b = TpdfDither::for_stream(44_100, 48_000, 1);
        let same = (0..256).filter(|_| a.sample() == b.sample()).count();
        assert_eq!(same, 0, "two streams shared {same} dither samples");
    }
}
