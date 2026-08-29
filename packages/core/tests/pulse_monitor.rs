use on_air_core::pipeline::capture::parse_pactl_monitor_sources;
use on_air_core::pipeline::local_sink::{is_local_hardware_sink, parse_sink_names};

#[test]
fn pactl_short_lists_analog_monitor() {
    let text = "\
40\teffect_input.macbookpro11_dsp.monitor\tPipeWire\tfloat32le 2ch 48000Hz\tSUSPENDED
62\talsa_output.pci-0000_00_1b.0.analog-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED
63\talsa_input.pci-0000_00_1b.0.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED
";
    let names = parse_pactl_monitor_sources(text);
    assert_eq!(
        names,
        vec![
            "effect_input.macbookpro11_dsp.monitor",
            "alsa_output.pci-0000_00_1b.0.analog-stereo.monitor",
        ]
    );
}

#[test]
fn pactl_short_names_include_analog_and_bluez() {
    let text = "\
40\teffect_input.macbookpro11_dsp\tPipeWire\tfloat32le 2ch 48000Hz\tSUSPENDED
62\talsa_output.pci-0000_00_1b.0.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING
5585\tbluez_output.58_EA_1F_87_56_45.1\tPipeWire\ts16le 2ch 48000Hz\tRUNNING
";
    assert_eq!(
        parse_sink_names(text),
        vec![
            "effect_input.macbookpro11_dsp",
            "alsa_output.pci-0000_00_1b.0.analog-stereo",
            "bluez_output.58_EA_1F_87_56_45.1",
        ]
    );
}

#[test]
fn dsp_and_bluez_are_not_treated_as_laptop_speakers() {
    assert!(is_local_hardware_sink(
        "alsa_output.pci-0000_00_1b.0.analog-stereo"
    ));
    assert!(is_local_hardware_sink("alsa_output.pci-0000_00_1b.0.hdmi-stereo"));
    assert!(!is_local_hardware_sink("effect_input.macbookpro11_dsp"));
    assert!(!is_local_hardware_sink("bluez_output.58_EA_1F_87_56_45.1"));
    assert!(!is_local_hardware_sink("easyeffects_sink"));
}
