use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, InputCallbackInfo, SampleFormat, Stream};
use ringbuf::{traits::Producer, HeapProd};

#[derive(Debug, Clone, PartialEq)]
pub struct InputDeviceInfo {
    pub name: String,
}

/// Loopback-oriented capture backend for the current OS (M5).
pub fn loopback_backend() -> &'static str {
    if cfg!(target_os = "linux") {
        "pipewire-monitor"
    } else if cfg!(target_os = "macos") {
        "coreaudio-screencapturekit"
    } else if cfg!(target_os = "windows") {
        "wasapi-loopback"
    } else {
        "cpal-default"
    }
}

pub fn list_input_devices(host: &Host) -> Result<Vec<InputDeviceInfo>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices
        .map(|d| InputDeviceInfo {
            name: d.to_string(),
        })
        .collect())
}

pub fn find_input_device(host: &Host, name: &str) -> Result<Option<Device>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices.into_iter().find(|d| d.to_string() == name))
}

/// Opens the device's default input config and starts pushing captured
/// samples into `producer`, downmixing to mono. Returns the running
/// `Stream` — dropping it stops capture (cpal's Drop impl joins its
/// worker thread, so drop it via `spawn_blocking` from async code).
pub fn start_capture(device: &Device, mut producer: HeapProd<f32>) -> Result<Stream, cpal::Error> {
    let supported = device.default_input_config()?;
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let config = supported.config();

    let err_fn = |err: cpal::Error| {
        eprintln!("capture stream error: {err}");
    };

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &InputCallbackInfo| {
                if channels <= 1 {
                    producer.push_slice(data);
                } else {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    producer.push_slice(&mono);
                }
            },
            err_fn,
            None,
        )?,
        other => {
            return Err(cpal::Error::with_message(
                cpal::ErrorKind::UnsupportedConfig,
                format!("unsupported input sample format: {other:?}"),
            ))
        }
    };

    stream.play()?;
    Ok(stream)
}
