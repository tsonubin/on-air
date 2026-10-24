//! Linux's Pulse protocol is also provided by pipewire-pulse. Audio links
//! live in that server, never in the shell. No shell command strings run.
use on_air_api::PatchbaySide;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub label: String,
    pub side: PatchbaySide,
    pub volume: u8,
    pub muted: bool,
    pub monitor_of: Option<String>,
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires an isolated PulseAudio server via PULSE_SERVER"]
    async fn real_server_creates_routes_and_recovers_owned_modules() {
        assert!(
            std::env::var("PULSE_SERVER").is_ok(),
            "use an isolated server"
        );
        let mut pulse = Pulse::new(false);
        let before = pulse.catalog().await.unwrap();
        let first = pulse.bus("test-input", "Test input").await.unwrap();
        let second = pulse.bus("test-output", "Test output").await.unwrap();
        let key = pulse
            .link(&format!("{first}.monitor"), &second, 50)
            .await
            .unwrap();
        pulse
            .control(PatchbaySide::Output, &second, "volume", Some("37%".into()))
            .await
            .unwrap();
        let catalog = pulse.catalog().await.unwrap();
        assert_eq!(
            catalog
                .devices
                .iter()
                .find(|d| d.name == second)
                .unwrap()
                .volume,
            37
        );
        assert!(catalog.running_sinks.contains(&second));
        let inputs = pulse.json("sink-inputs").await.unwrap();
        let input = inputs
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["properties"]["on_air_key"] == key)
            .unwrap();
        assert!(input["volume"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v["value"].as_u64() == Some(32768)));

        let mut restarted = Pulse::new(false);
        restarted.catalog().await.unwrap();
        assert_eq!(
            restarted.bus("test-input", "Test input").await.unwrap(),
            first
        );
        assert!(restarted.modules.contains_key(&key));
        restarted.retain_links(&[]).await.unwrap();
        restarted.unload(&first).await.unwrap();
        restarted.unload(&second).await.unwrap();
        let after = restarted.catalog().await.unwrap();
        assert!(!after
            .devices
            .iter()
            .any(|d| d.name == first || d.name == second));
        assert_eq!(before.default_output, after.default_output);
    }
}

#[derive(Clone, Default)]
pub struct Catalog {
    pub devices: Vec<Device>,
    pub default_input: String,
    pub default_output: String,
    pub running_sinks: Vec<String>,
}

pub struct Pulse {
    mock: bool,
    modules: HashMap<String, u32>,
    fake: Catalog,
}

pub fn token(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn bus_name(id: &str) -> String {
    format!("on_air_{}", token(id))
}

impl Pulse {
    pub fn new(mock: bool) -> Self {
        Self {
            mock,
            modules: HashMap::new(),
            fake: Catalog {
                devices: vec![
                    Device {
                        name: "mic".into(),
                        label: "USB mic".into(),
                        side: PatchbaySide::Input,
                        volume: 80,
                        muted: false,
                        monitor_of: None,
                    },
                    Device {
                        name: "headphones".into(),
                        label: "Headphones".into(),
                        side: PatchbaySide::Output,
                        volume: 64,
                        muted: false,
                        monitor_of: None,
                    },
                ],
                default_input: "mic".into(),
                default_output: "headphones".into(),
                ..Catalog::default()
            },
        }
    }

    async fn run(&self, args: &[String]) -> Result<String, String> {
        if !crate::platform::PATCHBAY {
            return Err("The patchbay requires Linux with pipewire-pulse or PulseAudio.".into());
        }
        let mut command = tokio::process::Command::new("pactl");
        command.args(args).env("LC_ALL", "C").kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(4), command.output())
            .await
            .map_err(|_| "The audio server did not respond.".to_string())?
            .map_err(|error| {
                format!("Install libpulse (pactl) and start pipewire-pulse: {error}")
            })?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    async fn json(&self, what: &str) -> Result<Value, String> {
        let text = self
            .run(&["--format=json".into(), "list".into(), what.into()])
            .await?;
        serde_json::from_str(&text)
            .map_err(|error| format!("Invalid audio server response: {error}"))
    }

    pub async fn catalog(&mut self) -> Result<Catalog, String> {
        if self.mock {
            return Ok(self.fake.clone());
        }
        let sinks = self.json("sinks").await?;
        let sources = self.json("sources").await?;
        let inputs = self.json("sink-inputs").await?;
        // pactl 17's JSON module list omits indices; its short format keeps
        // the IDs on both PulseAudio and pipewire-pulse.
        let modules = self
            .run(&["list".into(), "modules".into(), "short".into()])
            .await?;
        // Recover only our tagged modules after a core restart. Module IDs
        // are ephemeral, so always refresh them from the current server.
        self.modules.clear();
        for line in modules.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() < 3 {
                continue;
            }
            let (name, args) = (fields[1], fields[2]);
            if let Ok(index) = fields[0].parse::<u32>() {
                if let Some((_, suffix)) = args.split_once("on_air_key=") {
                    let key: String = suffix
                        .chars()
                        .take_while(|c| c.is_ascii_hexdigit())
                        .collect();
                    if key.len() == 16 {
                        self.modules.insert(key, index);
                    }
                }
                // Null sinks use a deterministic, owned sink name.
                if name == "module-null-sink" && args.contains("on_air.owner=patchbay") {
                    if let Some(name) = args
                        .split_whitespace()
                        .find_map(|part| part.strip_prefix("sink_name="))
                    {
                        self.modules.insert(name.into(), index);
                    }
                }
            }
        }
        let mut catalog = Catalog {
            // A running server can legitimately have no hardware/default.
            default_input: self
                .run(&["get-default-source".into()])
                .await
                .unwrap_or_default(),
            default_output: self
                .run(&["get-default-sink".into()])
                .await
                .unwrap_or_default(),
            ..Catalog::default()
        };
        for (list, side) in [
            (&sinks, PatchbaySide::Output),
            (&sources, PatchbaySide::Input),
        ] {
            for device in list.as_array().into_iter().flatten() {
                let Some(name) = device["name"].as_str() else {
                    continue;
                };
                let values: Vec<f64> = device["volume"]
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.values())
                    .filter_map(|v| v["value"].as_f64())
                    .collect();
                let volume = if values.is_empty() {
                    100
                } else {
                    ((values.iter().sum::<f64>() / values.len() as f64 / 65536.0) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                };
                let monitor_of = device["monitor_of_sink_name"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| name.strip_suffix(".monitor").map(str::to_string));
                catalog.devices.push(Device {
                    name: name.into(),
                    label: device["description"].as_str().unwrap_or(name).into(),
                    side,
                    volume,
                    muted: device["mute"].as_bool().unwrap_or(false),
                    monitor_of,
                });
            }
        }
        for input in inputs.as_array().into_iter().flatten() {
            if input["corked"].as_bool().unwrap_or(false) {
                continue;
            }
            if let Some(index) = input["sink"].as_u64() {
                if let Some(sink) = sinks
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|s| s["index"].as_u64() == Some(index))
                {
                    if let Some(name) = sink["name"].as_str() {
                        catalog.running_sinks.push(name.into());
                    }
                }
            }
        }
        Ok(catalog)
    }

    pub async fn bus(&mut self, id: &str, label: &str) -> Result<String, String> {
        let name = bus_name(id);
        if self.modules.contains_key(&name) {
            return Ok(name);
        }
        if self.mock {
            self.fake.devices.push(Device {
                name: name.clone(),
                label: label.into(),
                side: PatchbaySide::Output,
                volume: 64,
                muted: false,
                monitor_of: None,
            });
            self.fake.devices.push(Device {
                name: format!("{name}.monitor"),
                label: label.into(),
                side: PatchbaySide::Input,
                volume: 100,
                muted: false,
                monitor_of: Some(name.clone()),
            });
            self.modules
                .insert(name.clone(), self.modules.len() as u32 + 1);
            return Ok(name);
        }
        // A proplist quoted for Pulse's argument parser, not for a shell.
        let label: String = label
            .chars()
            .filter(|c| !c.is_control() && !matches!(c, '\'' | '"' | '\\'))
            .take(80)
            .collect();
        let id = self.run(&["load-module".into(), "module-null-sink".into(), format!("sink_name={name}"),
            "rate=44100".into(), "channels=2".into(),
            format!("sink_properties=\"device.description='{label} (On-Air)' on_air.owner=patchbay priority.session=0\"")]).await?;
        self.modules.insert(
            name.clone(),
            id.parse()
                .map_err(|_| "Invalid audio module ID".to_string())?,
        );
        Ok(name)
    }

    pub async fn link(&mut self, source: &str, sink: &str, gain: u8) -> Result<String, String> {
        let key = token(&format!("{source}\0{sink}\0{gain}"));
        if let Some(id) = self.modules.get(&key).copied() {
            if !self.mock && gain < 100 {
                self.link_gain(id, gain).await?;
            }
            return Ok(key);
        }
        let id = if self.mock {
            self.modules.len() as u32 + 1
        } else {
            self.run(&[
                "load-module".into(),
                "module-loopback".into(),
                format!("source={source}"),
                format!("sink={sink}"),
                "source_dont_move=true".into(),
                "sink_dont_move=true".into(),
                "latency_msec=30".into(),
                format!("sink_input_properties=\"on_air_key={key}\""),
            ])
            .await?
            .parse()
            .map_err(|_| "Invalid audio module ID".to_string())?
        };
        self.modules.insert(key.clone(), id);
        if !self.mock && gain < 100 {
            self.link_gain(id, gain).await?;
        }
        Ok(key)
    }

    async fn link_gain(&self, id: u32, gain: u8) -> Result<(), String> {
        let inputs = self.json("sink-inputs").await?;
        let input = inputs
            .as_array()
            .into_iter()
            .flatten()
            .find(|input| {
                input["owner_module"]
                    .as_u64()
                    .or_else(|| input["owner_module"].as_str().and_then(|s| s.parse().ok()))
                    == Some(u64::from(id))
            })
            .and_then(|input| input["index"].as_u64())
            .ok_or("The mix stream was not created by the audio server.")?;
        self.run(&[
            "set-sink-input-volume".into(),
            input.to_string(),
            format!("{gain}%"),
        ])
        .await?;
        Ok(())
    }

    pub async fn retain_links(&mut self, wanted: &[String]) -> Result<(), String> {
        let remove: Vec<_> = self
            .modules
            .keys()
            .filter(|k| !k.starts_with("on_air_") && !wanted.contains(k))
            .cloned()
            .collect();
        for key in remove {
            self.unload(&key).await?;
        }
        Ok(())
    }

    pub async fn unload(&mut self, key: &str) -> Result<(), String> {
        if let Some(id) = self.modules.get(key).copied() {
            if !self.mock {
                self.run(&["unload-module".into(), id.to_string()]).await?;
            }
            self.modules.remove(key);
            if self.mock {
                self.fake
                    .devices
                    .retain(|d| d.name != key && d.name != format!("{key}.monitor"));
            }
        }
        Ok(())
    }

    pub async fn control(
        &mut self,
        side: PatchbaySide,
        name: &str,
        action: &str,
        value: Option<String>,
    ) -> Result<(), String> {
        if self.mock {
            if action == "default" {
                if side == PatchbaySide::Input {
                    self.fake.default_input = name.into();
                } else {
                    self.fake.default_output = name.into();
                }
            }
            for device in &mut self.fake.devices {
                if device.name == name && device.side == side {
                    if action == "volume" {
                        device.volume = value
                            .as_deref()
                            .unwrap_or("0")
                            .trim_end_matches('%')
                            .parse()
                            .unwrap_or(0);
                    }
                    if action == "mute" {
                        device.muted = value.as_deref() == Some("1");
                    }
                }
            }
            return Ok(());
        }
        let noun = if side == PatchbaySide::Input {
            "source"
        } else {
            "sink"
        };
        let command = if action == "default" {
            format!("set-default-{noun}")
        } else {
            format!("set-{noun}-{action}")
        };
        let mut args = vec![command, name.into()];
        args.extend(value);
        self.run(&args).await.map(|_| ())
    }
}
