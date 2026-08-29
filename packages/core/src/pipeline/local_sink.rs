//! Keep laptop *hardware* quiet while a remote exclusive output is live.
//!
//! Never becomes the Pulse default sink and never mutes DSP/effect graphs
//! (e.g. `effect_input.macbookpro11_dsp`). Bluetooth audio is `pacat
//! --device=bluez_…` only.

use std::process::Command;
use std::sync::Mutex;

static MUTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SAVED_DEFAULT: Mutex<Option<String>> = Mutex::new(None);

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

fn pactl(args: &[&str]) -> bool {
    Command::new("pactl").args(args).status().map(|s| s.success()).unwrap_or(false)
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
    let output = Command::new("pactl").args(["get-default-sink"]).output().ok()?;
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

/// Remember the user's default sink if it is not already a Bluetooth speaker.
pub fn pin_default_sink() {
    let Some(name) = current_default_sink() else {
        return;
    };
    if name.to_ascii_lowercase().contains("bluez") {
        return;
    }
    let mut saved = SAVED_DEFAULT.lock().unwrap();
    if saved.is_none() {
        *saved = Some(name);
    }
}

/// If PipeWire auto-switched default output to A2DP, put it back.
pub fn restore_default_sink() {
    let Some(name) = SAVED_DEFAULT.lock().unwrap().clone() else {
        return;
    };
    let _ = pactl(&["set-default-sink", &name]);
}

pub fn forget_pinned_default() {
    *SAVED_DEFAULT.lock().unwrap() = None;
}

/// Mute laptop analog/HDMI only. Leaves DSP graphs and the default sink alone.
pub fn silence_local_speakers(keep: Option<&str>) {
    restore_muted_only();
    pin_default_sink();
    let mut muted = Vec::new();
    for name in listed_sinks() {
        if keep.is_some_and(|k| name == k) {
            continue;
        }
        if !is_local_hardware_sink(&name) {
            continue;
        }
        if pactl(&["set-sink-mute", &name, "1"]) {
            muted.push(name);
        }
    }
    *MUTED.lock().unwrap() = muted;
    restore_default_sink();
}

fn restore_muted_only() {
    let muted = std::mem::take(&mut *MUTED.lock().unwrap());
    for name in muted {
        let _ = pactl(&["set-sink-mute", &name, "0"]);
    }
}

pub fn restore_local_speakers() {
    restore_muted_only();
    restore_default_sink();
    forget_pinned_default();
}

pub fn apply_for_transport(transport: &str, device_id: &str) {
    let keep = match transport {
        "bluetooth" => Some(device_id),
        "sonos" | "airplay" => None,
        _ => return,
    };
    silence_local_speakers(keep);
}
