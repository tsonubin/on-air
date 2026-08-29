use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, InputCallbackInfo, SampleFormat, Stream};
use ringbuf::{traits::Producer, HeapProd};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

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

fn pulse_monitor_source_names() -> Vec<String> {
    let output = match Command::new("timeout")
        .args(["2", "pactl", "list", "sources", "short"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_pactl_monitor_sources(&String::from_utf8_lossy(&output.stdout))
}

/// Prefer the analog-output monitor over DSP/effect monitors.
pub fn preferred_pulse_monitor() -> Option<String> {
    let names = pulse_monitor_source_names();
    names
        .iter()
        .find(|n| n.contains("analog") && n.ends_with(".monitor"))
        .cloned()
        .or_else(|| names.into_iter().next())
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
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "parec stdout missing".to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while !stop_thread.load(Ordering::Relaxed) {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let samples = n / 4;
                    if samples == 0 {
                        continue;
                    }
                    let mut f32s = Vec::with_capacity(samples);
                    for chunk in buf[..samples * 4].chunks_exact(4) {
                        f32s.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                    }
                    producer.push_slice(&f32s);
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

pub fn find_preferred_loopback(host: &Host) -> Result<Option<Device>, cpal::Error> {
    let devices = host.input_devices()?;
    Ok(devices.into_iter().find(|d| is_loopback_device_name(&d.to_string())))
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

pub fn supported_input_rates_for_name(name: &str) -> Vec<(u32, u32)> {
    let host = cpal::default_host();
    find_input_device(&host, name)
        .ok()
        .flatten()
        .map(|device| supported_input_rate_ranges(&device))
        .unwrap_or_default()
}

/// Opens the device's default input config and starts pushing captured
/// samples into `producer`, downmixing to mono. Returns the running
/// `Stream` — dropping it stops capture (cpal's Drop impl joins its
/// worker thread, so drop it via `spawn_blocking` from async code).
pub fn start_capture(device: &Device, producer: HeapProd<f32>) -> Result<Stream, cpal::Error> {
    start_capture_at(device, producer, None).map(|(stream, _rate)| stream)
}

/// Like [`start_capture`], but tries `preferred_rate_hz` first so the user-
/// selected input rate can be the native capture rate (no extra resample).
/// Returns the actual capture sample rate the stream is running at.
pub fn start_capture_at(
    device: &Device,
    mut producer: HeapProd<f32>,
    preferred_rate_hz: Option<u32>,
) -> Result<(Stream, u32), cpal::Error> {
    let supported = device.default_input_config()?;
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let mut config = supported.config();
    if let Some(rate) = preferred_rate_hz {
        let supported_here = supported_input_rate_ranges(device)
            .into_iter()
            .any(|(min, max)| rate >= min && rate <= max);
        if supported_here {
            config.sample_rate = rate;
        }
    }
    let actual_rate = config.sample_rate;

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
    Ok((stream, actual_rate))
}
