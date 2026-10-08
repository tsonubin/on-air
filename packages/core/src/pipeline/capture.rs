use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, FromSample, Host, InputCallbackInfo, Sample, SampleFormat, SizedSample, Stream,
    StreamConfig, I24, U24,
};
use ringbuf::{traits::Producer, HeapProd};

#[derive(Debug, Clone, PartialEq)]
pub struct InputDeviceInfo {
    pub name: String,
}

/// Loopback-oriented capture backend for the current OS (M5).
pub fn is_loopback_device_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("monitor")
        || n.contains("loopback")
        || n.contains("blackhole")
        || n.contains("soundflower")
        || n.contains("stereo mix")
        || n.contains("what u hear")
        || n.contains("screencapture")
        || n.contains("wasapi")
}

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
    let mut listed: Vec<InputDeviceInfo> = devices
        .map(|d| InputDeviceInfo {
            name: d.to_string(),
        })
        .collect();
    for name in pulse_monitor_source_names() {
        if !listed.iter().any(|d| d.name == name) {
            listed.push(InputDeviceInfo { name });
        }
    }
    listed.sort_by_key(|d| !is_loopback_device_name(&d.name));
    Ok(listed)
}

/// `pactl list sources short` lines whose name ends in `.monitor`.
pub fn parse_pactl_monitor_sources(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        let name = match cols.len() {
            0 => continue,
            1 => cols[0],
            _ => {
                if cols[0].chars().all(|c| c.is_ascii_digit()) {
                    cols[1]
                } else {
                    cols[0]
                }
            }
        };
        if name.ends_with(".monitor") {
            names.push(name.to_string());
        }
    }
    names
}

pub fn is_pulse_monitor_name(name: &str) -> bool {
    name.contains(".monitor")
}

/// Linux only: PulseAudio/PipeWire monitor sources via `pactl`, and
/// capture through `parec`. Other platforms never spawn these binaries.
#[cfg(target_os = "linux")]
mod pulse {
    use super::parse_pactl_monitor_sources;
    use ringbuf::{traits::Producer, HeapProd};
    use std::io::Read;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;

    pub fn pulse_monitor_source_names() -> Vec<String> {
        let output = match Command::new("timeout")
            .args(["2", "pactl", "list", "sources", "short"])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        parse_pactl_monitor_sources(&String::from_utf8_lossy(&output.stdout))
    }

    pub struct PulseMonitorCapture {
        stop: Arc<AtomicBool>,
        child: Option<Child>,
        reader: Option<JoinHandle<()>>,
    }

    impl PulseMonitorCapture {
        pub fn stop(mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }

    /// Capture a Pulse/PipeWire monitor source (`*.monitor`) via `parec`.
    pub fn start_pulse_monitor(
        source: &str,
        mut producer: HeapProd<f32>,
        preferred_rate_hz: Option<u32>,
    ) -> Result<(PulseMonitorCapture, u32), String> {
        let rate = preferred_rate_hz.unwrap_or(48000);
        let mut child = Command::new("parec")
            .args([
                "--device",
                source,
                "--file-format=raw",
                "--format=float32le",
                "--rate",
                &rate.to_string(),
                "--channels=1",
                "--latency-msec=50",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("parec: {e}"))?;
        let mut stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "parec stdout missing".to_string()
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let reader = std::thread::spawn(move || {
            // Keep up to three bytes between reads because pipe reads are not
            // guaranteed to end on an f32 sample boundary.
            let mut buf = [0u8; 4099];
            let mut pending = 0;
            while !stop_thread.load(Ordering::Relaxed) {
                match stdout.read(&mut buf[pending..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        let total = pending + n;
                        let complete_bytes = total - (total % 4);
                        let (samples, _) = buf[..complete_bytes].as_chunks::<4>();
                        producer
                            .push_iter(samples.iter().map(|sample| f32::from_le_bytes(*sample)));
                        pending = total - complete_bytes;
                        buf.copy_within(complete_bytes..total, 0);
                    }
                    Err(_) => break,
                }
            }
        });
        Ok((
            PulseMonitorCapture {
                stop,
                child: Some(child),
                reader: Some(reader),
            },
            rate,
        ))
    }
}

/// No-op stand-ins so the rest of the pipeline compiles unchanged where
/// PulseAudio does not exist.
#[cfg(not(target_os = "linux"))]
mod pulse {
    use ringbuf::HeapProd;

    pub fn pulse_monitor_source_names() -> Vec<String> {
        Vec::new()
    }

    pub struct PulseMonitorCapture;

    impl PulseMonitorCapture {
        pub fn stop(self) {}
    }

    pub fn start_pulse_monitor(
        _source: &str,
        _producer: HeapProd<f32>,
        _preferred_rate_hz: Option<u32>,
    ) -> Result<(PulseMonitorCapture, u32), String> {
        Err("Pulse monitor capture is only available on Linux".to_string())
    }
}

use pulse::pulse_monitor_source_names;
pub use pulse::{start_pulse_monitor, PulseMonitorCapture};

/// Prefer the analog-output monitor over DSP/effect monitors.
pub fn preferred_pulse_monitor() -> Option<String> {
    let names = pulse_monitor_source_names();
    names
        .iter()
        .find(|n| n.contains("analog") && n.ends_with(".monitor"))
        .cloned()
        .or_else(|| names.into_iter().next())
}

pub fn find_preferred_loopback(host: &Host) -> Result<Option<Device>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices
        .into_iter()
        .find(|d| is_loopback_device_name(&d.to_string())))
}

pub fn find_input_device(host: &Host, name: &str) -> Result<Option<Device>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices.into_iter().find(|d| d.to_string() == name))
}

/// `(min_hz, max_hz)` ranges advertised by a capture device.
pub fn supported_input_rate_ranges(device: &Device) -> Vec<(u32, u32)> {
    match device.supported_input_configs() {
        Ok(cfgs) => cfgs
            .map(|cfg| (cfg.min_sample_rate(), cfg.max_sample_rate()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Opens an input config on `device` and starts pushing captured samples
/// into `producer`, downmixing to mono. `preferred_rate_hz` is tried first
/// so the user-selected input rate can be the native capture rate (no extra
/// resample). Returns the running `Stream` (dropping it stops capture; cpal's
/// Drop joins its worker thread, so drop it via `spawn_blocking` from async
/// code) and the actual capture sample rate.
pub fn start_capture_at(
    device: &Device,
    producer: HeapProd<f32>,
    preferred_rate_hz: Option<u32>,
) -> Result<(Stream, u32), cpal::Error> {
    let default = device.default_input_config()?;
    let supported = preferred_rate_hz
        .and_then(|rate| {
            device.supported_input_configs().ok().and_then(|configs| {
                configs
                    .filter(|config| {
                        rate >= config.min_sample_rate() && rate <= config.max_sample_rate()
                    })
                    .min_by_key(|config| {
                        (
                            config.sample_format() != default.sample_format(),
                            config.channels().abs_diff(default.channels()),
                        )
                    })
                    .map(|config| config.with_sample_rate(rate))
            })
        })
        .unwrap_or(default);
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let config = supported.config();
    let actual_rate = config.sample_rate;

    let stream = match sample_format {
        SampleFormat::I8 => build_input_stream::<i8>(device, config, channels, producer)?,
        SampleFormat::I16 => build_input_stream::<i16>(device, config, channels, producer)?,
        SampleFormat::I24 => build_input_stream::<I24>(device, config, channels, producer)?,
        SampleFormat::I32 => build_input_stream::<i32>(device, config, channels, producer)?,
        SampleFormat::I64 => build_input_stream::<i64>(device, config, channels, producer)?,
        SampleFormat::U8 => build_input_stream::<u8>(device, config, channels, producer)?,
        SampleFormat::U16 => build_input_stream::<u16>(device, config, channels, producer)?,
        SampleFormat::U24 => build_input_stream::<U24>(device, config, channels, producer)?,
        SampleFormat::U32 => build_input_stream::<u32>(device, config, channels, producer)?,
        SampleFormat::U64 => build_input_stream::<u64>(device, config, channels, producer)?,
        SampleFormat::F32 => build_input_stream::<f32>(device, config, channels, producer)?,
        SampleFormat::F64 => build_input_stream::<f64>(device, config, channels, producer)?,
        other => {
            return Err(cpal::Error::with_message(
                cpal::ErrorKind::UnsupportedConfig,
                format!("unsupported input sample format: {other:?}"),
            ))
        }
    };

    stream.play()?;
    Ok((stream, actual_rate))
}

fn build_input_stream<T>(
    device: &Device,
    config: StreamConfig,
    channels: usize,
    mut producer: HeapProd<f32>,
) -> Result<Stream, cpal::Error>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _: &InputCallbackInfo| {
            if channels <= 1 {
                producer.push_iter(data.iter().copied().map(f32::from_sample));
                return;
            }

            for frame in data.chunks_exact(channels) {
                let sample =
                    frame.iter().copied().map(f32::from_sample).sum::<f32>() / channels as f32;
                if producer.try_push(sample).is_err() {
                    break;
                }
            }
        },
        |err| eprintln!("capture stream error: {err}"),
        None,
    )
}
