//! Platform Bluetooth adapters. Audio still plays through an OS sink;
//! this module discovers, pairs, and connects those sinks.

#[cfg(target_os = "linux")]
use super::{pulse_a2dp_sinks, pulse_sink_address};
use super::{BluetoothDevice, BluetoothEndpoint};
use crate::sender::SenderError;
use std::collections::BTreeMap;

pub fn list() -> Vec<BluetoothDevice> {
    platform::list()
}

pub fn pair(id: &str) -> Result<(), SenderError> {
    platform::pair(id)
}

pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
    platform::connect(id)
}

pub fn open_settings() -> Result<(), SenderError> {
    platform::open_settings()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothctlInfo {
    pub paired: bool,
    pub connected: bool,
    pub audio_sink: bool,
}

pub fn parse_bluetoothctl_devices(text: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Device ") else {
            continue;
        };
        let Some((addr, name)) = rest.split_once(' ') else {
            continue;
        };
        if addr.chars().filter(|c| *c == ':').count() != 5 {
            continue;
        }
        devices.push((addr.to_ascii_uppercase(), name.trim().to_string()));
    }
    devices
}

pub fn parse_bluetoothctl_info(text: &str) -> BluetoothctlInfo {
    let mut info = BluetoothctlInfo {
        paired: false,
        connected: false,
        audio_sink: false,
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "Paired: yes" {
            info.paired = true;
        } else if trimmed == "Connected: yes" {
            info.connected = true;
        } else if trimmed.contains("Audio Sink")
            || trimmed.contains("0000110b-")
            || trimmed.contains("0000110d-")
        {
            info.audio_sink = true;
        }
    }
    info
}

/// CoreAudio `kAudioDeviceTransportType*` four-char codes.
pub fn is_coreaudio_bluetooth_transport(code: u32) -> bool {
    const BLUE: u32 = u32::from_be_bytes(*b"blue");
    const BLEA: u32 = u32::from_be_bytes(*b"blea");
    code == BLUE || code == BLEA
}

pub fn looks_like_windows_bluetooth_id(id: &str) -> bool {
    let n = id.to_ascii_uppercase();
    n.contains("BTHENUM") || n.contains("BTHHFENUM") || n.contains("BTHLE")
}

fn merge_by_id(devices: Vec<BluetoothDevice>) -> Vec<BluetoothDevice> {
    let mut by_id: BTreeMap<String, BluetoothDevice> = BTreeMap::new();
    for device in devices {
        by_id
            .entry(device.id.clone())
            .and_modify(|existing| {
                if existing.name.starts_with("bluez_output.") && !device.name.starts_with("bluez") {
                    existing.name = device.name.clone();
                }
                existing.paired = existing.paired || device.paired;
                existing.connected = existing.connected || device.connected;
                if existing.audio_endpoint.is_none() {
                    existing.audio_endpoint = device.audio_endpoint.clone();
                }
            })
            .or_insert(device);
    }
    by_id.into_values().collect()
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    fn bluetoothctl(args: &[&str]) -> String {
        Command::new("bluetoothctl")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    }

    pub fn list() -> Vec<BluetoothDevice> {
        let mut devices = pulse_a2dp_sinks();
        for (addr, name) in parse_bluetoothctl_devices(&bluetoothctl(&["devices", "Paired"])) {
            let info = parse_bluetoothctl_info(&bluetoothctl(&["info", &addr]));
            if !info.audio_sink && info.paired {
                // Some adapters omit UUID text until connected; keep paired devices
                // that already have a Pulse sink, skip obvious non-audio otherwise.
                if !devices.iter().any(|device| device.id == addr) {
                    continue;
                }
            }
            devices.push(BluetoothDevice {
                id: addr,
                name,
                paired: true,
                connected: info.connected,
                audio_endpoint: None,
            });
        }
        for (addr, name) in parse_bluetoothctl_devices(&bluetoothctl(&["devices"])) {
            if devices.iter().any(|device| device.id == addr) {
                continue;
            }
            let info = parse_bluetoothctl_info(&bluetoothctl(&["info", &addr]));
            if !info.audio_sink && info.paired {
                continue;
            }
            if !info.paired && !info.audio_sink {
                continue;
            }
            devices.push(BluetoothDevice {
                id: addr,
                name,
                paired: info.paired,
                connected: info.connected,
                audio_endpoint: None,
            });
        }
        merge_by_id(devices)
    }

    fn preserving_default_sink<T>(
        operation: impl FnOnce() -> Result<T, SenderError>,
    ) -> Result<T, SenderError> {
        // Pairing as well as connecting can make PipeWire switch the default.
        crate::pipeline::local_sink::pin_default_sink();
        let result = operation();
        crate::pipeline::local_sink::restore_default_sink();
        result
    }

    pub fn pair(id: &str) -> Result<(), SenderError> {
        preserving_default_sink(|| {
            let addr = pulse_sink_address(id).unwrap_or_else(|| id.to_ascii_uppercase());
            let output = Command::new("bluetoothctl")
                .args(["--timeout", "30", "pair", &addr])
                .output()
                .map_err(|error| {
                    SenderError::transport(format!("bluetoothctl pair failed: {error}"))
                })?;
            if !output.status.success() {
                return Err(SenderError::transport(format!(
                    "could not pair {addr}: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            connect(&addr).map(|_| ())
        })
    }

    pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
        preserving_default_sink(|| {
            let addr = pulse_sink_address(id).unwrap_or_else(|| id.to_ascii_uppercase());
            let endpoint = if let Some(endpoint) = connected_endpoint(&addr) {
                // Playback through Pulse does not require bluetoothctl when
                // another application already connected the speaker.
                endpoint
            } else {
                let status = Command::new("bluetoothctl")
                    .args(["--timeout", "15", "connect", &addr])
                    .status()
                    .map_err(|e| {
                        SenderError::transport(format!("bluetoothctl connect failed: {e}"))
                    })?;
                if !status.success() {
                    return Err(SenderError::transport(format!(
                        "could not connect Bluetooth device {addr}"
                    )));
                }
                wait_for_sink(&addr)?
            };
            let status = Command::new("pactl")
                .args(["set-sink-mute", &endpoint, "0"])
                .status()
                .map_err(|e| {
                    SenderError::transport(format!("could not unmute Bluetooth output: {e}"))
                })?;
            if !status.success() {
                return Err(SenderError::transport("could not unmute Bluetooth output"));
            }
            Ok(BluetoothEndpoint::Pulse(endpoint))
        })
    }

    fn connected_endpoint(addr: &str) -> Option<String> {
        pulse_a2dp_sinks()
            .into_iter()
            .find(|device| device.id == addr)
            .and_then(|device| device.audio_endpoint)
    }

    fn wait_for_sink(addr: &str) -> Result<String, SenderError> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(endpoint) = connected_endpoint(addr) {
                return Ok(endpoint);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(SenderError::not_ready(format!(
            "bluetooth device {addr} did not appear as an audio sink"
        )))
    }

    pub fn open_settings() -> Result<(), SenderError> {
        const CANDIDATES: &[(&str, &[&str])] = &[
            ("gnome-control-center", &["bluetooth"]),
            ("blueman-manager", &[]),
            ("kcmshell6", &["kcm_bluetooth"]),
            ("kcmshell5", &["bluedevildevices"]),
        ];
        for (bin, args) in CANDIDATES {
            if Command::new(bin).args(*args).spawn().is_ok() {
                return Ok(());
            }
        }
        Err(SenderError::internal(
            "could not open Bluetooth settings (tried GNOME, Blueman, KDE)",
        ))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::process::Command;

    pub fn list() -> Vec<BluetoothDevice> {
        super::merge_by_id(super::macos::audio_devices())
    }

    pub fn pair(_id: &str) -> Result<(), SenderError> {
        open_settings()
    }

    pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
        super::macos::connect(id)
    }

    pub fn open_settings() -> Result<(), SenderError> {
        let opened = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.BluetoothSettings")
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if opened {
            return Ok(());
        }
        let legacy = Command::new("open")
            .arg("/System/Library/PreferencePanes/Bluetooth.prefPane")
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if legacy {
            Ok(())
        } else {
            Err(SenderError::internal("could not open Bluetooth settings"))
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn list() -> Vec<BluetoothDevice> {
        super::merge_by_id(super::windows::audio_devices())
    }

    pub fn pair(_id: &str) -> Result<(), SenderError> {
        open_settings()
    }

    pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
        super::windows::connect(id)
    }

    pub fn open_settings() -> Result<(), SenderError> {
        let status = Command::new("cmd")
            .args(["/C", "start", "ms-settings:bluetooth"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|error| {
                SenderError::internal(format!("could not open Bluetooth settings: {error}"))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(SenderError::internal("could not open Bluetooth settings"))
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;

    pub fn list() -> Vec<BluetoothDevice> {
        Vec::new()
    }

    pub fn pair(_id: &str) -> Result<(), SenderError> {
        Err(SenderError::not_ready("bluetooth pairing is not supported"))
    }

    pub fn connect(_id: &str) -> Result<BluetoothEndpoint, SenderError> {
        Err(SenderError::not_ready("bluetooth connect is not supported"))
    }

    pub fn open_settings() -> Result<(), SenderError> {
        Err(SenderError::not_ready(
            "bluetooth settings are not supported",
        ))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2_core_audio::{
        kAudioDevicePropertyDeviceName, kAudioDevicePropertyScopeOutput,
        kAudioDevicePropertyStreamConfiguration, kAudioDevicePropertyTransportType,
        kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
        AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
    };
    use objc2_core_audio_types::AudioBufferList;
    use std::ffi::CStr;
    use std::ptr::{self, NonNull};

    fn property(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    fn get_data(
        device: AudioObjectID,
        address: &AudioObjectPropertyAddress,
        out: &mut [u8],
    ) -> bool {
        let mut size = out.len() as u32;
        let Some(data) = NonNull::new(out.as_mut_ptr().cast()) else {
            return false;
        };
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(address),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                data,
            )
        };
        status == 0
    }

    fn data_size(device: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<u32> {
        let mut size = 0u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                device,
                NonNull::from(address),
                0,
                ptr::null(),
                NonNull::from(&mut size),
            )
        };
        (status == 0 && size > 0).then_some(size)
    }

    fn transport_type(device: AudioObjectID) -> Option<u32> {
        let address = property(
            kAudioDevicePropertyTransportType,
            kAudioObjectPropertyScopeGlobal,
        );
        let mut bytes = [0u8; 4];
        if !get_data(device, &address, &mut bytes) {
            return None;
        }
        Some(u32::from_ne_bytes(bytes))
    }

    fn device_name(device: AudioObjectID) -> Option<String> {
        let address = property(
            kAudioDevicePropertyDeviceName,
            kAudioObjectPropertyScopeGlobal,
        );
        let size = data_size(device, &address)? as usize;
        let mut buf = vec![0u8; size.max(1)];
        if !get_data(device, &address, &mut buf) {
            return None;
        }
        let name = CStr::from_bytes_until_nul(&buf).ok()?.to_string_lossy();
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    }

    fn has_output(device: AudioObjectID) -> bool {
        let address = property(
            kAudioDevicePropertyStreamConfiguration,
            kAudioDevicePropertyScopeOutput,
        );
        let Some(size) = data_size(device, &address) else {
            return false;
        };
        if (size as usize) <= std::mem::size_of::<u32>() {
            return false;
        }
        let mut buf = vec![0u8; size as usize];
        if !get_data(device, &address, &mut buf) {
            return false;
        }
        // `buf` is a Vec<u8> with no alignment guarantee, so never form a
        // reference to the AudioBufferList inside it: read unaligned copies
        // of the fields instead.
        let list = buf.as_ptr() as *const AudioBufferList;
        // SAFETY: CoreAudio filled `buf` with an AudioBufferList of at least
        // `size` bytes, and `mNumberBuffers` entries follow the header.
        let number_buffers =
            unsafe { std::ptr::read_unaligned(std::ptr::addr_of!((*list).mNumberBuffers)) };
        (0..number_buffers).any(|i| {
            let buffer = unsafe {
                std::ptr::read_unaligned(
                    std::ptr::addr_of!((*list).mBuffers)
                        .cast::<objc2_core_audio_types::AudioBuffer>()
                        .add(i as usize),
                )
            };
            buffer.mNumberChannels > 0
        })
    }

    fn system_devices() -> Vec<AudioObjectID> {
        let address = property(
            kAudioHardwarePropertyDevices,
            kAudioObjectPropertyScopeGlobal,
        );
        let system = kAudioObjectSystemObject as AudioObjectID;
        let Some(size) = data_size(system, &address) else {
            return Vec::new();
        };
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut devices = vec![0u32; count];
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                devices.as_mut_ptr() as *mut u8,
                count * std::mem::size_of::<AudioObjectID>(),
            )
        };
        if !get_data(system, &address, bytes) {
            Vec::new()
        } else {
            devices
        }
    }

    pub fn audio_devices() -> Vec<BluetoothDevice> {
        let mut devices = Vec::new();
        for id in system_devices() {
            let Some(code) = transport_type(id) else {
                continue;
            };
            if !is_coreaudio_bluetooth_transport(code) {
                continue;
            }
            if !has_output(id) {
                continue;
            }
            let Some(name) = device_name(id) else {
                continue;
            };
            devices.push(BluetoothDevice {
                id: name.clone(),
                name: name.clone(),
                paired: true,
                connected: true,
                audio_endpoint: Some(name),
            });
        }
        devices
    }

    pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
        audio_devices()
            .into_iter()
            .find(|device| device.id == id || device.audio_endpoint.as_deref() == Some(id))
            .and_then(|device| device.audio_endpoint)
            .map(BluetoothEndpoint::Native)
            .ok_or_else(|| {
                SenderError::not_ready(
                    "bluetooth speaker is not connected — pair it in Bluetooth settings first",
                )
            })
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{looks_like_windows_bluetooth_id, BluetoothDevice, BluetoothEndpoint, SenderError};
    use ::windows::core::{BSTR, GUID};
    use ::windows::Win32::Media::Audio::{
        eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE,
        DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
    };
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    };
    use ::windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    fn init_com() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn enumerator() -> Option<IMMDeviceEnumerator> {
        init_com();
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()
    }

    fn device_id(device: &IMMDevice) -> Option<String> {
        let id = unsafe { device.GetId() }.ok()?;
        // GetId allocates a null-terminated string with the COM task allocator.
        let result = unsafe { id.to_string() }.ok();
        unsafe { CoTaskMemFree(Some(id.0.cast())) };
        result
    }

    fn device_name(device: &IMMDevice) -> Option<String> {
        let store = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
        // PKEY_Device_FriendlyName
        let key = PROPERTYKEY {
            fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
            pid: 14,
        };
        let value = unsafe { store.GetValue(&key) }.ok()?;
        Some(BSTR::try_from(&value).ok()?.to_string())
    }

    pub fn audio_devices() -> Vec<BluetoothDevice> {
        let Some(enumerator) = enumerator() else {
            return Vec::new();
        };
        let Ok(collection) = (unsafe {
            enumerator.EnumAudioEndpoints(
                eRender,
                DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_UNPLUGGED.0),
            )
        }) else {
            return Vec::new();
        };
        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        let mut devices = Vec::new();
        for index in 0..count {
            let Ok(device) = (unsafe { collection.Item(index) }) else {
                continue;
            };
            let Some(id) = device_id(&device) else {
                continue;
            };
            if !looks_like_windows_bluetooth_id(&id) {
                continue;
            }
            let name = device_name(&device).unwrap_or_else(|| id.clone());
            devices.push(BluetoothDevice {
                id: id.clone(),
                name: name.clone(),
                paired: true,
                connected: true,
                audio_endpoint: Some(name),
            });
        }
        devices
    }

    pub fn connect(id: &str) -> Result<BluetoothEndpoint, SenderError> {
        audio_devices()
            .into_iter()
            .find(|device| device.id == id || device.audio_endpoint.as_deref() == Some(id))
            .and_then(|device| device.audio_endpoint)
            .map(BluetoothEndpoint::Native)
            .ok_or_else(|| {
                SenderError::not_ready(
                    "bluetooth speaker is not connected — pair it in Bluetooth settings first",
                )
            })
    }
}
