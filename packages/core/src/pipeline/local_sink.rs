//! Keep laptop *hardware* quiet while a remote exclusive output is live.
//!
//! Never becomes the Pulse default sink and never mutes DSP/effect graphs
//! (e.g. `effect_input.macbookpro11_dsp`). Bluetooth audio is `pacat
//! --device=bluez_…` only.

/// Built-in analog/HDMI only — not EasyEffects/DSP, null sinks, or A2DP.
pub fn is_local_hardware_sink(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("bluez") || n.contains("bluetooth") {
        return false;
    }
    if n.contains("effect") || n.contains("easyeffects") || n.contains("null") {
        return false;
    }
    n.contains("analog") || n.contains("hdmi") || n.starts_with("alsa_output.")
}

/// Pulse sink names from `pactl list sinks short`.
pub fn parse_sink_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        let name = if cols[0].chars().all(|c| c.is_ascii_digit()) {
            cols[1]
        } else {
            cols[0]
        };
        if !name.is_empty() {
            names.push(name.to_string());
        }
    }
    names
}

pub fn parse_mute_state(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "mute: yes" | "yes" | "1" | "true" => Some(true),
        "mute: no" | "no" | "0" | "false" => Some(false),
        _ => None,
    }
}

/// The `pactl` mute dance itself. Linux only: no other platform has
/// PulseAudio, and the laptop speakers there are left alone.
#[cfg(target_os = "linux")]
mod pulse {
    use super::{is_local_hardware_sink, parse_mute_state, parse_sink_names};
    use parking_lot::Mutex;
    use std::process::Command;

    static SAVED_MUTE_STATES: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
    static SAVED_DEFAULT: Mutex<Option<String>> = Mutex::new(None);

    fn pactl(args: &[&str]) -> bool {
        Command::new("pactl")
            .args(args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn listed_sinks() -> Vec<String> {
        let output = match Command::new("timeout")
            .args(["2", "pactl", "list", "sinks", "short"])
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        parse_sink_names(&String::from_utf8_lossy(&output.stdout))
    }

    fn current_default_sink() -> Option<String> {
        let output = Command::new("pactl")
            .args(["get-default-sink"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    fn sink_is_muted(name: &str) -> Option<bool> {
        let output = Command::new("pactl")
            .args(["get-sink-mute", name])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_mute_state(&String::from_utf8_lossy(&output.stdout))
    }

    /// Remember the user's default sink if it is not already a Bluetooth speaker.
    pub fn pin_default_sink() {
        let Some(name) = current_default_sink() else {
            return;
        };
        if name.to_ascii_lowercase().contains("bluez") {
            return;
        }
        let mut saved = SAVED_DEFAULT.lock();
        if saved.is_none() {
            *saved = Some(name);
        }
    }

    /// If PipeWire auto-switched default output to A2DP, put it back.
    pub fn restore_default_sink() {
        let Some(name) = SAVED_DEFAULT.lock().clone() else {
            return;
        };
        let _ = pactl(&["set-default-sink", &name]);
    }

    pub fn forget_pinned_default() {
        *SAVED_DEFAULT.lock() = None;
    }

    /// Mute laptop analog/HDMI only. Leaves DSP graphs and the default sink alone.
    pub fn silence_local_speakers(keep: Option<&str>) {
        restore_muted_only();
        pin_default_sink();
        let mut saved_states = Vec::new();
        for name in listed_sinks() {
            if keep.is_some_and(|k| name == k) {
                continue;
            }
            if !is_local_hardware_sink(&name) {
                continue;
            }
            let Some(was_muted) = sink_is_muted(&name) else {
                continue;
            };
            if was_muted || pactl(&["set-sink-mute", &name, "1"]) {
                saved_states.push((name, was_muted));
            }
        }
        *SAVED_MUTE_STATES.lock() = saved_states;
        restore_default_sink();
    }

    fn restore_muted_only() {
        let saved_states = std::mem::take(&mut *SAVED_MUTE_STATES.lock());
        for (name, was_muted) in saved_states {
            let value = if was_muted { "1" } else { "0" };
            let _ = pactl(&["set-sink-mute", &name, value]);
        }
    }

    pub fn restore_local_speakers() {
        restore_muted_only();
        restore_default_sink();
        forget_pinned_default();
    }
}

#[cfg(not(target_os = "linux"))]
mod pulse {
    pub fn pin_default_sink() {}
    pub fn restore_default_sink() {}
    pub fn forget_pinned_default() {}
    pub fn silence_local_speakers(_keep: Option<&str>) {}
    pub fn restore_local_speakers() {}
}

pub use pulse::{
    forget_pinned_default, pin_default_sink, restore_default_sink, restore_local_speakers,
    silence_local_speakers,
};

pub fn apply_for_transport(transport: &str, device_id: &str) {
    let keep = match transport {
        "bluetooth" => Some(device_id),
        "sonos" | "airplay" => None,
        _ => return,
    };
    silence_local_speakers(keep);
}
