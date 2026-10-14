//! Friendly names for capture devices, ported from the desktop GUI's
//! `prettyInput.ts` and `inputLabels.ts`. The raw name stays the key.

/// Matches the core's `AUDIO_CD_INPUT` and the GUI's constant.
const AUDIO_CD_INPUT: &str = "Audio CD";

/// A capture device's raw name and the label the TUI shows for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLabel {
    pub name: String,
    pub label: String,
}

/// Shortens a raw capture-device name. Unrecognised names are returned as-is.
pub fn pretty_input(name: &str) -> String {
    if name == AUDIO_CD_INPUT {
        return AUDIO_CD_INPUT.into();
    }
    if name.starts_with("Discard all samples") {
        return "Null device".into();
    }
    if name.contains("PipeWire Sound Server") {
        return "PipeWire".into();
    }
    if name.starts_with("Default ALSA") {
        return "Default".into();
    }
    if name.contains("analog") && name.ends_with(".monitor") {
        return "Analog monitor".into();
    }
    if name.ends_with(".monitor") {
        return "System monitor".into();
    }
    if name.contains("CS4208 Analog") {
        return "Built-in analog".into();
    }
    if name.contains("HDMI") {
        return name.replace("HDA Intel HDMI, ", "HDMI ");
    }
    name.into()
}

/// Dedupes devices by raw name, then labels each one. When different names
/// shorten to the same label, the later ones get a " (n)" suffix.
pub fn input_labels(names: &[String]) -> Vec<InputLabel> {
    let mut seen = std::collections::HashSet::new();
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut rows = Vec::new();
    for name in names {
        if !seen.insert(name.as_str()) {
            continue;
        }
        let base = pretty_input(name);
        let count = counts.entry(base.clone()).or_insert(0);
        *count += 1;
        let label = if *count == 1 {
            base
        } else {
            format!("{base} ({count})")
        };
        rows.push(InputLabel {
            name: name.clone(),
            label,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_common_capture_names() {
        assert_eq!(
            pretty_input("Discard all samples (playback)"),
            "Null device"
        );
        assert_eq!(
            pretty_input("alsa_output.pci.analog-stereo.monitor"),
            "Analog monitor"
        );
        assert_eq!(pretty_input("alsa_output.usb.monitor"), "System monitor");
        assert_eq!(pretty_input("PipeWire Sound Server"), "PipeWire");
        assert_eq!(pretty_input("Default ALSA Output"), "Default");
        assert_eq!(
            pretty_input("HDA Intel PCH, CS4208 Analog"),
            "Built-in analog"
        );
        assert_eq!(pretty_input("HDA Intel HDMI, HDMI 0"), "HDMI HDMI 0");
        assert_eq!(pretty_input("Audio CD"), "Audio CD");
        assert_eq!(pretty_input("USB mic"), "USB mic");
    }

    #[test]
    fn dedupes_names_and_suffixes_label_clashes() {
        let names: Vec<String> = ["a.monitor", "a.monitor", "b.monitor", "USB mic"]
            .map(String::from)
            .into();
        let labels: Vec<_> = input_labels(&names)
            .into_iter()
            .map(|l| (l.name, l.label))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("a.monitor".into(), "System monitor".into()),
                ("b.monitor".into(), "System monitor (2)".into()),
                ("USB mic".into(), "USB mic".into()),
            ]
        );
    }
}
