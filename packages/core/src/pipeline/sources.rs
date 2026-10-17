//! The input list a person picks from on Linux: what PipeWire (through its
//! Pulse API) reports as real capture sources, plus exactly one "System
//! audio" entry for the monitor of the default output. ALSA aliases, monitors
//! of other outputs and on-air's own patchbay buses never appear.
//!
//! Labels are what clients see and what settings remember; the Pulse source
//! name stays internal and is looked up again on every activation.

use serde_json::Value;

/// The label of the monitor of the default output.
pub const SYSTEM_AUDIO: &str = "System audio";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratedSource {
    /// Shown to people and remembered in settings.
    pub label: String,
    /// The Pulse source `parec --device` captures.
    pub source: String,
}

/// What `pactl --format=json list sources` and `pactl get-default-sink` say.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    raw: Vec<Value>,
    default_sink: Option<String>,
}

fn is_own_bus(name: &str) -> bool {
    name.starts_with("on_air_")
}

fn str_field<'a>(source: &'a Value, key: &str) -> &'a str {
    source.get(key).and_then(Value::as_str).unwrap_or("")
}

fn property<'a>(source: &'a Value, key: &str) -> &'a str {
    source
        .get("properties")
        .and_then(|p| p.get(key))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// The sink a monitor source belongs to, or `None` for a capture source.
fn monitored_sink(source: &Value) -> Option<&str> {
    let sink = str_field(source, "monitor_source");
    if !sink.is_empty() {
        Some(sink)
    } else if property(source, "device.class") == "monitor" {
        Some(str_field(source, "name").trim_end_matches(".monitor"))
    } else {
        None
    }
}

impl Sources {
    pub fn parse(sources_json: &str, default_sink: Option<&str>) -> Result<Self, String> {
        let raw = match serde_json::from_str(sources_json) {
            Ok(Value::Array(raw)) => raw,
            Ok(_) => return Err("pactl did not return a source list".into()),
            Err(error) => return Err(format!("invalid pactl source list: {error}")),
        };
        Ok(Self {
            raw,
            default_sink: default_sink
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Into::into),
        })
    }

    /// The monitor that best stands for "what this computer plays": the
    /// default output's, unless that is one of our own buses, then the first
    /// hardware output's.
    fn system_monitor(&self) -> Option<&str> {
        let monitors = || {
            self.raw
                .iter()
                .filter_map(|s| monitored_sink(s).map(|sink| (sink, str_field(s, "name"))))
                .filter(|(sink, _)| !is_own_bus(sink))
        };
        let default = self.default_sink.as_deref();
        monitors()
            .find(|(sink, _)| Some(*sink) == default)
            .or_else(|| {
                monitors().find(|(_, name)| {
                    self.raw.iter().any(|s| {
                        str_field(s, "name") == *name && !property(s, "device.api").is_empty()
                    })
                })
            })
            .or_else(|| monitors().next())
            .map(|(_, name)| name)
    }

    /// System audio first, then capture sources in the order PipeWire lists
    /// them. Two sources with the same description get " 2", " 3", ….
    pub fn curated(&self) -> Vec<CuratedSource> {
        let mut list = Vec::new();
        if let Some(source) = self.system_monitor() {
            list.push(CuratedSource {
                label: SYSTEM_AUDIO.into(),
                source: source.into(),
            });
        }
        for source in &self.raw {
            let name = str_field(source, "name");
            if name.is_empty() || is_own_bus(name) || monitored_sink(source).is_some() {
                continue;
            }
            let description = str_field(source, "description");
            let base = if description.is_empty() {
                name
            } else {
                description
            };
            let mut label = base.to_string();
            let mut n = 2;
            while list.iter().any(|c: &CuratedSource| c.label == label) {
                label = format!("{base} {n}");
                n += 1;
            }
            list.push(CuratedSource {
                label,
                source: name.into(),
            });
        }
        list
    }

    /// The Pulse source for a label, or for an exact Pulse source name (the
    /// patchbay captures its own buses this way, and older settings saved
    /// raw monitor names).
    pub fn resolve(&self, requested: &str) -> Option<String> {
        if let Some(found) = self.curated().into_iter().find(|c| c.label == requested) {
            return Some(found.source);
        }
        self.raw
            .iter()
            .map(|s| str_field(s, "name"))
            .find(|name| *name == requested)
            .map(Into::into)
    }

    /// The label to show for an activated source: its curated label, or the
    /// raw name when it is not on the curated list.
    pub fn label_for(&self, requested: &str) -> String {
        let curated = self.curated();
        if curated.iter().any(|c| c.label == requested) {
            return requested.into();
        }
        curated
            .into_iter()
            .find(|c| c.source == requested)
            .map(|c| c.label)
            .unwrap_or_else(|| requested.into())
    }
}

/// Ask the running Pulse server. `None` when `pactl` is missing or fails, so
/// callers fall back to the platform device list.
#[cfg(target_os = "linux")]
pub fn query() -> Option<Sources> {
    use std::process::Command;
    let run = |args: &[&str]| {
        Command::new("timeout")
            .arg("2")
            .arg("pactl")
            .args(args)
            .env("LC_ALL", "C")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let sources = run(&["--format=json", "list", "sources"])?;
    let default_sink = run(&["get-default-sink"]);
    Sources::parse(&sources, default_sink.as_deref()).ok()
}

#[cfg(not(target_os = "linux"))]
pub fn query() -> Option<Sources> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This machine's sources (a MacBook with a DSP chain as the default
    /// output and the patchbay's desktop bus), plus a USB interface, a
    /// virtual noise-suppressed mic and HDMI.
    const SOURCES: &str = r#"[
      {"name":"effect_input.macbookpro11_dsp.monitor","description":"Monitor of MacBookPro11 DSP",
       "monitor_source":"effect_input.macbookpro11_dsp","properties":{"device.class":"monitor","node.virtual":"true"}},
      {"name":"on_air_68693d02ab4fbb23.monitor","description":"Monitor of On-Air desktop (On-Air)",
       "monitor_source":"on_air_68693d02ab4fbb23","properties":{"device.class":"monitor"}},
      {"name":"alsa_output.pci-0000_00_1b.0.analog-stereo.monitor","description":"Monitor of Built-in Audio Analog Stereo",
       "monitor_source":"alsa_output.pci-0000_00_1b.0.analog-stereo","properties":{"device.class":"monitor","device.api":"alsa"}},
      {"name":"alsa_output.pci-0000_00_03.0.hdmi-stereo.monitor","description":"Monitor of Built-in Audio Digital Stereo (HDMI)",
       "monitor_source":"alsa_output.pci-0000_00_03.0.hdmi-stereo","properties":{"device.class":"monitor","device.api":"alsa"}},
      {"name":"alsa_input.pci-0000_00_1b.0.analog-stereo","description":"Built-in Audio Analog Stereo",
       "monitor_source":"","properties":{"device.class":"sound","device.api":"alsa"}},
      {"name":"alsa_input.usb-Focusrite_Scarlett_2i2-00.analog-stereo","description":"Scarlett 2i2 USB",
       "monitor_source":"","properties":{"device.class":"sound","device.api":"alsa","device.bus":"usb"}},
      {"name":"alsa_input.usb-Generic_USB_Audio-00.analog-stereo","description":"USB Audio",
       "monitor_source":"","properties":{"device.class":"sound","device.api":"alsa","device.bus":"usb"}},
      {"name":"alsa_input.usb-Generic_USB_Audio-01.analog-stereo","description":"USB Audio",
       "monitor_source":"","properties":{"device.class":"sound","device.api":"alsa","device.bus":"usb"}},
      {"name":"rnnoise_source","description":"Noise Canceling source",
       "monitor_source":"","properties":{"node.virtual":"true"}},
      {"name":"on_air_mix_1","description":"Mix 1 (On-Air)","monitor_source":"","properties":{}}
    ]"#;

    fn sources(default_sink: Option<&str>) -> Sources {
        Sources::parse(SOURCES, default_sink).unwrap()
    }

    fn labels(sources: &Sources) -> Vec<String> {
        sources.curated().into_iter().map(|c| c.label).collect()
    }

    #[test]
    fn lists_system_audio_once_then_capture_sources() {
        let sources = sources(Some("effect_input.macbookpro11_dsp"));
        assert_eq!(
            labels(&sources),
            [
                "System audio",
                "Built-in Audio Analog Stereo",
                "Scarlett 2i2 USB",
                "USB Audio",
                "USB Audio 2",
                "Noise Canceling source",
            ]
        );
    }

    #[test]
    fn system_audio_is_the_default_outputs_monitor() {
        let sources = sources(Some("effect_input.macbookpro11_dsp"));
        assert_eq!(
            sources.resolve(SYSTEM_AUDIO).as_deref(),
            Some("effect_input.macbookpro11_dsp.monitor")
        );
        let hdmi = self::sources(Some("alsa_output.pci-0000_00_03.0.hdmi-stereo"));
        assert_eq!(
            hdmi.resolve(SYSTEM_AUDIO).as_deref(),
            Some("alsa_output.pci-0000_00_03.0.hdmi-stereo.monitor")
        );
    }

    #[test]
    fn our_own_bus_as_default_falls_back_to_a_hardware_monitor() {
        let sources = sources(Some("on_air_68693d02ab4fbb23"));
        assert_eq!(
            sources.resolve(SYSTEM_AUDIO).as_deref(),
            Some("alsa_output.pci-0000_00_1b.0.analog-stereo.monitor")
        );
    }

    #[test]
    fn never_lists_our_buses_or_other_monitors() {
        let all = labels(&sources(None)).join("|");
        assert!(!all.contains("On-Air"), "{all}");
        assert!(!all.contains("Monitor of"), "{all}");
    }

    #[test]
    fn raw_source_names_still_resolve_and_map_to_labels() {
        let sources = sources(Some("effect_input.macbookpro11_dsp"));
        assert_eq!(
            sources
                .resolve("on_air_68693d02ab4fbb23.monitor")
                .as_deref(),
            Some("on_air_68693d02ab4fbb23.monitor")
        );
        assert_eq!(
            sources.label_for("effect_input.macbookpro11_dsp.monitor"),
            SYSTEM_AUDIO
        );
        assert_eq!(
            sources.label_for("alsa_input.usb-Generic_USB_Audio-01.analog-stereo"),
            "USB Audio 2"
        );
        assert_eq!(sources.resolve("HDA Intel PCH, CS4208 Analog"), None);
    }

    #[test]
    fn rejects_output_that_is_not_a_list() {
        assert!(Sources::parse("{}", None).is_err());
        assert!(Sources::parse("not json", None).is_err());
    }
}
