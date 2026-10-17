//! "This computer" outputs: the ports of the computer's own audio hardware
//! (built-in speakers, a headphone jack, HDMI), one entry per port that is
//! present now. Playback reuses the Bluetooth PCM path: pipeline audio is
//! written to a Pulse sink with `pacat`.
//!
//! Choosing a port switches the hardware sink to it. When the default output
//! is a virtual DSP chain in front of the only hardware sink (the MacBook
//! speaker EQ, EasyEffects), audio is played into that chain so it keeps
//! applying; otherwise it goes to the hardware sink directly.

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use crate::sender::bluetooth::{BluetoothAdapter, BluetoothDevice, BluetoothEndpoint};
use crate::sender::SenderError;

pub const TRANSPORT: &str = "local";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOutput {
    /// `<sink>` or `<sink>#<port>`; stable across restarts.
    pub id: String,
    pub name: String,
    /// The hardware sink that owns the port.
    pub sink: String,
    pub port: Option<String>,
    /// Where `pacat` plays: the sink, or the virtual chain in front of it.
    pub target: String,
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn property<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get("properties")
        .and_then(|p| p.get(key))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Local hardware: an ALSA (or other native) sink that is not Bluetooth and
/// not one of our patchbay buses.
fn is_local_hardware(sink: &Value) -> bool {
    let api = property(sink, "device.api");
    !api.is_empty() && api != "bluez5" && !str_field(sink, "name").starts_with("on_air_")
}

/// Parse `pactl --format=json list sinks` into the outputs a person can pick.
pub fn parse(sinks_json: &str, default_sink: Option<&str>) -> Result<Vec<LocalOutput>, String> {
    let sinks = match serde_json::from_str(sinks_json) {
        Ok(Value::Array(sinks)) => sinks,
        Ok(_) => return Err("pactl did not return a sink list".into()),
        Err(error) => return Err(format!("invalid pactl sink list: {error}")),
    };
    let hardware: Vec<&Value> = sinks.iter().filter(|s| is_local_hardware(s)).collect();
    let default_sink = default_sink.map(str::trim).filter(|s| !s.is_empty());
    let virtual_default = default_sink.filter(|name| {
        sinks.iter().any(|s| {
            str_field(s, "name") == *name
                && property(s, "node.virtual") == "true"
                && !name.starts_with("on_air_")
        })
    });

    let mut outputs = Vec::new();
    for sink in &hardware {
        let sink_name = str_field(sink, "name");
        let target = match virtual_default {
            Some(chain) if hardware.len() == 1 => chain,
            _ => sink_name,
        };
        let ports: Vec<&Value> = sink
            .get("ports")
            .and_then(Value::as_array)
            .map(|ports| {
                ports
                    .iter()
                    .filter(|p| str_field(p, "availability") != "not available")
                    .collect()
            })
            .unwrap_or_default();
        if ports.is_empty() {
            outputs.push(LocalOutput {
                id: sink_name.into(),
                name: str_field(sink, "description").into(),
                sink: sink_name.into(),
                port: None,
                target: target.into(),
            });
            continue;
        }
        for port in ports {
            let port_name = str_field(port, "name");
            outputs.push(LocalOutput {
                id: format!("{sink_name}#{port_name}"),
                name: str_field(port, "description").into(),
                sink: sink_name.into(),
                port: Some(port_name.into()),
                target: target.into(),
            });
        }
    }
    // Two sinks may both have a "Speakers" port; say whose.
    let names: Vec<String> = outputs.iter().map(|o| o.name.clone()).collect();
    for output in &mut outputs {
        if names.iter().filter(|n| **n == output.name).count() > 1 {
            if let Some(sink) = hardware
                .iter()
                .find(|s| str_field(s, "name") == output.sink)
            {
                output.name = format!("{} ({})", output.name, str_field(sink, "description"));
            }
        }
    }
    Ok(outputs)
}

#[cfg(target_os = "linux")]
fn pactl(args: &[&str]) -> Option<String> {
    std::process::Command::new("timeout")
        .arg("2")
        .arg("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The computer's own outputs, or nothing when there is no Pulse server.
#[cfg(target_os = "linux")]
pub fn query() -> Vec<LocalOutput> {
    let Some(sinks) = pactl(&["--format=json", "list", "sinks"]) else {
        return Vec::new();
    };
    let default_sink = pactl(&["get-default-sink"]);
    parse(&sinks, default_sink.as_deref()).unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
pub fn query() -> Vec<LocalOutput> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn active_port(sink: &str) -> Option<String> {
    let sinks: Value = serde_json::from_str(&pactl(&["--format=json", "list", "sinks"])?).ok()?;
    sinks
        .as_array()?
        .iter()
        .find(|s| str_field(s, "name") == sink)
        .map(|s| str_field(s, "active_port").to_string())
        .filter(|p| !p.is_empty())
}

#[cfg(target_os = "linux")]
fn set_port(sink: &str, port: &str) -> Result<(), SenderError> {
    pactl(&["set-sink-port", sink, port])
        .map(|_| ())
        .ok_or_else(|| SenderError::not_ready(format!("could not switch {sink} to {port}")))
}

#[cfg(not(target_os = "linux"))]
fn active_port(_sink: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "linux"))]
fn set_port(_sink: &str, _port: &str) -> Result<(), SenderError> {
    Err(SenderError::not_ready(
        "local outputs need PipeWire or PulseAudio",
    ))
}

/// Lets the PCM sender drive a local port: "connecting" switches the port
/// and names the Pulse target; "disconnecting" puts the old port back.
pub struct LocalPortAdapter {
    output: LocalOutput,
    previous_port: Mutex<Option<String>>,
}

impl LocalPortAdapter {
    pub fn new(output: LocalOutput) -> Arc<Self> {
        Arc::new(Self {
            output,
            previous_port: Mutex::new(None),
        })
    }

    /// The sender's view of the output; it only needs the id and name.
    pub fn device(&self) -> BluetoothDevice {
        BluetoothDevice {
            id: self.output.id.clone(),
            name: self.output.name.clone(),
            paired: true,
            connected: true,
            audio_endpoint: Some(self.output.target.clone()),
        }
    }
}

impl BluetoothAdapter for LocalPortAdapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        vec![self.device()]
    }

    fn pair(&self, _id: &str) -> Result<(), SenderError> {
        Ok(())
    }

    fn connect(&self, _id: &str) -> Result<BluetoothEndpoint, SenderError> {
        if let Some(port) = &self.output.port {
            let previous = active_port(&self.output.sink);
            if previous.as_deref() != Some(port.as_str()) {
                set_port(&self.output.sink, port)?;
                *self.previous_port.lock() = previous;
            }
        }
        Ok(BluetoothEndpoint::Pulse(self.output.target.clone()))
    }

    fn disconnect(&self, _id: &str) -> Result<(), SenderError> {
        if let Some(previous) = self.previous_port.lock().take() {
            set_port(&self.output.sink, &previous)?;
        }
        Ok(())
    }

    fn open_settings(&self) -> Result<(), SenderError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This machine: the MacBook DSP chain as default output, the patchbay's
    /// desktop bus, the built-in card with speakers and an unplugged jack.
    const MACBOOK: &str = r#"[
      {"name":"effect_input.macbookpro11_dsp","description":"MacBookPro11 DSP","ports":[],
       "properties":{"node.virtual":"true","media.class":"Audio/Sink"}},
      {"name":"on_air_68693d02ab4fbb23","description":"On-Air desktop (On-Air)","ports":[],"properties":{}},
      {"name":"alsa_output.pci-0000_00_1b.0.analog-stereo","description":"Built-in Audio Analog Stereo",
       "active_port":"analog-output-speaker",
       "ports":[
         {"name":"analog-output-speaker","description":"Speakers","availability":"availability unknown"},
         {"name":"analog-output-headphones","description":"Headphones","availability":"not available"}],
       "properties":{"device.api":"alsa"}}
    ]"#;

    fn names(outputs: &[LocalOutput]) -> Vec<&str> {
        outputs.iter().map(|o| o.name.as_str()).collect()
    }

    #[test]
    fn lists_present_ports_through_the_dsp_chain() {
        let outputs = parse(MACBOOK, Some("effect_input.macbookpro11_dsp")).unwrap();
        assert_eq!(
            outputs,
            [LocalOutput {
                id: "alsa_output.pci-0000_00_1b.0.analog-stereo#analog-output-speaker".into(),
                name: "Speakers".into(),
                sink: "alsa_output.pci-0000_00_1b.0.analog-stereo".into(),
                port: Some("analog-output-speaker".into()),
                target: "effect_input.macbookpro11_dsp".into(),
            }]
        );
    }

    #[test]
    fn a_plugged_in_jack_adds_headphones() {
        let plugged = MACBOOK.replace("\"not available\"", "\"available\"");
        let outputs = parse(&plugged, Some("effect_input.macbookpro11_dsp")).unwrap();
        assert_eq!(names(&outputs), ["Speakers", "Headphones"]);
    }

    #[test]
    fn plays_to_the_hardware_sink_without_a_dsp_default() {
        let outputs = parse(MACBOOK, Some("alsa_output.pci-0000_00_1b.0.analog-stereo")).unwrap();
        assert_eq!(
            outputs[0].target,
            "alsa_output.pci-0000_00_1b.0.analog-stereo"
        );
        // Our own bus as the default is not a DSP chain either.
        let outputs = parse(MACBOOK, Some("on_air_68693d02ab4fbb23")).unwrap();
        assert_eq!(
            outputs[0].target,
            "alsa_output.pci-0000_00_1b.0.analog-stereo"
        );
    }

    #[test]
    fn skips_bluetooth_and_names_duplicate_ports_by_card() {
        let sinks = r#"[
          {"name":"alsa_output.pci-a","description":"Built-in Audio","ports":[
            {"name":"speaker","description":"Speakers","availability":"available"}],"properties":{"device.api":"alsa"}},
          {"name":"alsa_output.usb-b","description":"USB DAC","ports":[
            {"name":"speaker","description":"Speakers","availability":"available"}],"properties":{"device.api":"alsa"}},
          {"name":"alsa_output.pci-c.hdmi","description":"HDMI","ports":[],"properties":{"device.api":"alsa"}},
          {"name":"bluez_output.AA_BB","description":"JBL Flip 6","ports":[],"properties":{"device.api":"bluez5"}}
        ]"#;
        let outputs = parse(sinks, None).unwrap();
        assert_eq!(
            names(&outputs),
            ["Speakers (Built-in Audio)", "Speakers (USB DAC)", "HDMI"]
        );
        assert!(outputs.iter().all(|o| o.target == o.sink));
    }

    #[test]
    fn rejects_output_that_is_not_a_list() {
        assert!(parse("{}", None).is_err());
    }
}
