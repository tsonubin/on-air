/// Pipeline PCM is interleaved stereo: every frame is `[left, right]`, in the
/// capture ring, on the bus and in the radio WAV.
pub const PIPELINE_CHANNELS: usize = 2;

pub use on_air_api::DspQuality;

pub mod bridge;
pub mod channel;
pub mod eq;
pub mod quality;
pub mod rates;
pub mod resample;
