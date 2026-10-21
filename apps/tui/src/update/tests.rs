use super::*;
use crate::api::{
    ActiveOutput, ActiveOutputView, CdStatus, OutputInfo, OutputPhase, SampleRateResponse,
    SampleRateSide, StatusResponse,
};
use crate::model::DestState;

fn output(id: &str, name: &str, transport: &str, needs_pair: bool, paired: bool) -> OutputInfo {
    OutputInfo {
        id: id.into(),
        name: name.into(),
        transport: transport.into(),
        kind: "solo".into(),
        member_count: 1,
        needs_pair,
        paired,
    }
}

fn model_with_mock_snapshot() -> Model {
    let mut model = Model::new();
    model.snapshot = Snapshot {
        status: Some(StatusResponse {
            status: "ok".into(),
            version: "0.1.0".into(),
            service_enabled: true,
            sleep_inhibited: Some(true),
            lan_addresses: Some(vec!["10.0.0.12".into()]),
            firewall_warning: None,
            capture_buffer_ms: None,
        }),
        pin: Some("123456".into()),
        inputs: vec!["Mock Monitor".into()],
        active_input: None,
        outputs: vec![
            output("uuid:mock-sonos", "Mock Sonos", "sonos", false, true),
            output("ap-living", "Living Room AirPlay", "airplay", false, true),
            output(
                "bt-speaker",
                "Mock Bluetooth Speaker",
                "bluetooth",
                false,
                true,
            ),
            output(
                "bt-unpaired",
                "New Bluetooth Speaker",
                "bluetooth",
                true,
                false,
            ),
        ],
        active_output: None,
        volume: 50,
        eq: [0.0; 5],
        rates: Some(SampleRateResponse {
            sample_rate_hz: 48000,
            input: SampleRateSide {
                sample_rate_hz: 48000,
                supported_hz: vec![44100, 48000],
                transport: None,
            },
            output: SampleRateSide {
                sample_rate_hz: 44100,
                supported_hz: vec![44100, 48000, 96000],
                transport: Some("sonos".into()),
            },
        }),
        cd: None,
        airplay_mode: Some("owntone".into()),
        bluetooth: None,
    };
    model.autostart = Some(false);
    model
}

/// Acknowledge every mixer write, dropping what the acks send.
fn settle(m: &mut Model) {
    for control in [
        MixerControl::Volume,
        MixerControl::Eq,
        MixerControl::InputRate,
        MixerControl::OutputRate,
    ] {
        m.writes[control as usize] = WriteSlot::Idle;
    }
}

fn written(control: MixerControl, result: Result<(), String>) -> Msg {
    Msg::Written { control, result }
}

fn key(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn code(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn live_sonos() -> ActiveOutputView {
    ActiveOutputView {
        error: None,
        output: ActiveOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into(),
            device_name: "Mock Sonos".into(),
        },
        state: Some(OutputPhase::Live),
    }
}

fn disc(playing: bool) -> CdStatus {
    CdStatus {
        present: true,
        playing,
        track: 3,
        track_count: 12,
        title: Some("Track title".into()),
        album: None,
        position_ms: 134_000,
        duration_ms: 270_000,
        tracks: vec![],
    }
}

#[test]
fn tab_cycles_panes_and_wraps() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(m.pane, Pane::Sources);
    update(&mut m, code(KeyCode::Tab));
    assert_eq!(m.pane, Pane::Outputs);
    update(&mut m, code(KeyCode::Tab));
    assert_eq!(m.pane, Pane::Mixer);
    update(&mut m, code(KeyCode::Tab));
    assert_eq!(m.pane, Pane::Sources);
    update(&mut m, code(KeyCode::BackTab));
    assert_eq!(m.pane, Pane::Mixer);
}

#[test]
fn j_and_k_skip_output_headers() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Outputs;
    let id = |m: &Model| m.selected_dest().map(|e| e.id);
    assert!(matches!(m.dest_rows()[0], DestRow::Header("Sonos")));
    assert_eq!(id(&m).as_deref(), Some("uuid:mock-sonos"));
    update(&mut m, key('j'));
    assert_eq!(
        id(&m).as_deref(),
        Some("ap-living"),
        "skips the AirPlay header"
    );
    update(&mut m, key('k'));
    assert_eq!(id(&m).as_deref(), Some("uuid:mock-sonos"));
    update(&mut m, key('k'));
    assert_eq!(
        id(&m).as_deref(),
        Some("uuid:mock-sonos"),
        "never lands on a header"
    );
}

#[test]
fn enter_on_a_source_activates_it() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateInput("Mock Monitor".into())]
    );
    assert_eq!(m.pending.as_ref().unwrap().label, "Switching source");
}

#[test]
fn source_rows_skip_duplicate_names() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.inputs = vec!["a.monitor".into(), "a.monitor".into(), "USB mic".into()];
    update(&mut m, key('j'));
    update(&mut m, key('j'));
    assert_eq!(m.source_index, 1);
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateInput("USB mic".into())]
    );
}

#[test]
fn enter_on_a_paired_output_activates_it() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Outputs;
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into()
        }]
    );
    assert_eq!(m.pending.as_ref().unwrap().label, "Switching output");
}

#[test]
fn enter_on_an_unpaired_bluetooth_output_opens_the_pin_modal() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Outputs;
    for _ in 0..3 {
        update(&mut m, key('j'));
    }
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(
        m.modal,
        Modal::Pin {
            target: PairTarget::Bluetooth {
                id: "bt-unpaired".into(),
                name: "New Bluetooth Speaker".into()
            },
            input: String::new(),
            error: None
        }
    );
}

#[test]
fn pin_modal_accepts_digits_submits_on_enter_and_closes_on_esc() {
    let mut m = model_with_mock_snapshot();
    m.modal = Modal::Pin {
        target: PairTarget::Airplay {
            device_id: "ap-new".into(),
            name: "HomePod".into(),
        },
        input: String::new(),
        error: None,
    };
    update(&mut m, key('a'));
    for c in "1234".chars() {
        update(&mut m, key(c));
    }
    assert!(matches!(&m.modal, Modal::Pin { input, .. } if input == "1234"));
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairAirplay {
            device_id: "ap-new".into(),
            pin: "1234".into()
        }]
    );
    assert_eq!(m.pending.as_ref().unwrap().label, "Pairing");
    update(&mut m, code(KeyCode::Esc));
    assert_eq!(m.modal, Modal::None);

    m.modal = Modal::Pin {
        target: PairTarget::Bluetooth {
            id: "bt-unpaired".into(),
            name: "New".into(),
        },
        input: String::new(),
        error: None,
    };
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairBluetooth("bt-unpaired".into())],
        "bluetooth needs no PIN"
    );
}

#[test]
fn a_failed_pairing_keeps_the_modal_with_the_error() {
    let mut m = model_with_mock_snapshot();
    m.modal = Modal::Pin {
        target: PairTarget::Airplay {
            device_id: "ap-new".into(),
            name: "HomePod".into(),
        },
        input: "9999".into(),
        error: None,
    };
    m.pending = Some(Pending {
        label: "Pairing".into(),
    });
    m.activation = Some(Activation {
        transport: "airplay".into(),
        id: "ap-new".into(),
        name: "HomePod".into(),
        step: ActivationStep::Pairing,
    });
    let copy = "That PIN did not match. Check the code on the speaker and try again.";
    assert_eq!(
        update(
            &mut m,
            Msg::Done {
                label: "Pairing".into(),
                result: Err(copy.into())
            }
        ),
        vec![]
    );
    assert!(matches!(&m.modal, Modal::Pin { error: Some(e), .. } if e == copy));
    assert!(m.pending.is_none());
}

fn bt(id: &str, paired: bool, connected: bool) -> crate::api::BluetoothDeviceInfo {
    crate::api::BluetoothDeviceInfo {
        id: id.into(),
        name: id.into(),
        paired,
        connected,
    }
}

fn on_dest(id: &str) -> Model {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Outputs;
    m.snapshot.bluetooth = Some(vec![
        bt("bt-speaker", true, true),
        bt("bt-unpaired", false, false),
    ]);
    while m.selected_dest().map(|e| e.id) != Some(id.into()) {
        update(&mut m, key('j'));
    }
    m
}

fn done(label: &str, result: Result<(), String>) -> Msg {
    Msg::Done {
        label: label.into(),
        result,
    }
}

fn step(m: &Model) -> Option<ActivationStep> {
    m.activation.as_ref().map(|a| a.step)
}

#[test]
fn enter_on_ready_output_activates() {
    let mut m = on_dest("uuid:mock-sonos");
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into()
        }]
    );
    assert_eq!(step(&m), Some(ActivationStep::Activating));
    assert_eq!(m.pending.as_ref().unwrap().label, LABEL_OUTPUT);
}

#[test]
fn enter_on_airplay_pin_opens_pin_modal() {
    let mut m = on_dest("ap-living");
    m.snapshot.outputs[1].needs_pair = true;
    m.snapshot.outputs[1].paired = false;
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(matches!(
        &m.modal,
        Modal::Pin {
            target: PairTarget::Airplay { .. },
            ..
        }
    ));
    for c in "1234".chars() {
        update(&mut m, key(c));
    }
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairAirplay {
            device_id: "ap-living".into(),
            pin: "1234".into()
        }]
    );
    assert_eq!(step(&m), Some(ActivationStep::Pairing));
    assert_eq!(
        update(&mut m, done(LABEL_PAIR, Ok(()))),
        vec![Cmd::ActivateOutput {
            transport: "airplay".into(),
            device_id: "ap-living".into()
        }]
    );
    assert_eq!(step(&m), Some(ActivationStep::Activating));
}

#[test]
fn enter_on_failed_airplay_row_that_needs_a_pin_reopens_the_modal() {
    let mut m = on_dest("ap-living");
    m.snapshot.outputs[1].needs_pair = true;
    m.snapshot.outputs[1].paired = false;
    m.failed_output = Some(("airplay".into(), "ap-living".into()));
    assert_eq!(m.selected_dest().unwrap().state, DestState::Failed);
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(matches!(
        &m.modal,
        Modal::Pin {
            target: PairTarget::Airplay { .. },
            ..
        }
    ));
    assert_eq!(m.failed_output, None);
}

#[test]
fn enter_on_unpaired_bluetooth_confirms_then_pairs_connects_activates() {
    let mut m = on_dest("bt-unpaired");
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(matches!(
        &m.modal,
        Modal::Pin {
            target: PairTarget::Bluetooth { .. },
            ..
        }
    ));
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairBluetooth("bt-unpaired".into())]
    );
    assert_eq!(step(&m), Some(ActivationStep::Pairing));
    assert_eq!(
        update(&mut m, done(LABEL_PAIR, Ok(()))),
        vec![Cmd::ConnectBluetooth("bt-unpaired".into())]
    );
    assert_eq!(m.modal, Modal::None);
    assert_eq!(step(&m), Some(ActivationStep::Connecting));
    assert_eq!(m.pending.as_ref().unwrap().label, LABEL_CONNECT);
    assert_eq!(
        update(&mut m, done(LABEL_CONNECT, Ok(()))),
        vec![Cmd::ActivateOutput {
            transport: "bluetooth".into(),
            device_id: "bt-unpaired".into()
        }]
    );
    assert_eq!(step(&m), Some(ActivationStep::Activating));
    assert_eq!(
        update(&mut m, done(LABEL_OUTPUT, Ok(()))),
        vec![Cmd::Refresh]
    );
    assert_eq!(m.activation, None);
}

#[test]
fn enter_on_paired_bluetooth_connects_first() {
    let mut m = on_dest("bt-speaker");
    m.snapshot.bluetooth = Some(vec![
        bt("bt-speaker", true, false),
        bt("bt-unpaired", false, false),
    ]);
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ConnectBluetooth("bt-speaker".into())]
    );
    assert_eq!(m.modal, Modal::None);
    assert_eq!(step(&m), Some(ActivationStep::Connecting));
}

#[test]
fn enter_on_connected_bluetooth_activates() {
    let mut m = on_dest("bt-speaker");
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateOutput {
            transport: "bluetooth".into(),
            device_id: "bt-speaker".into()
        }]
    );
}

#[test]
fn second_enter_while_busy_is_refused() {
    let mut m = on_dest("uuid:mock-sonos");
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, key('j'));
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(m.notice.as_deref(), Some("Already connecting Mock Sonos…"));
}

#[test]
fn activation_failure_marks_the_row_and_enter_retries() {
    let mut m = on_dest("uuid:mock-sonos");
    update(&mut m, code(KeyCode::Enter));
    assert_eq!(
        update(&mut m, done(LABEL_OUTPUT, Err("boom".into()))),
        vec![Cmd::Refresh]
    );
    assert_eq!(
        m.failed_output,
        Some(("sonos".into(), "uuid:mock-sonos".into()))
    );
    assert_eq!(m.activation, None);
    assert_eq!(m.error.as_deref(), Some("boom"));
    assert_eq!(m.selected_dest().unwrap().state, DestState::Failed);
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into()
        }]
    );
    assert_eq!(m.failed_output, None);
}

#[test]
fn pair_failure_stays_in_the_modal() {
    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, done(LABEL_PAIR, Err("no luck".into())));
    assert!(matches!(&m.modal, Modal::Pin { error: Some(e), .. } if e == "no luck"));
    update(&mut m, code(KeyCode::Esc));
    assert_eq!(m.modal, Modal::None);
    assert_eq!(m.activation, None);
}

#[test]
fn done_for_a_vanished_row_clears_state() {
    let mut m = on_dest("uuid:mock-sonos");
    update(&mut m, code(KeyCode::Enter));
    m.snapshot.outputs.remove(0);
    update(&mut m, done(LABEL_OUTPUT, Err("gone".into())));
    assert_eq!(m.activation, None);
    assert_eq!(
        m.failed_output,
        Some(("sonos".into(), "uuid:mock-sonos".into()))
    );
    assert!(m
        .dest_rows()
        .iter()
        .all(|r| !matches!(r, DestRow::Entry(e) if e.id == "uuid:mock-sonos")));
}

#[test]
fn h_and_l_adjust_the_focused_mixer_row() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    assert_eq!(update(&mut m, key('l')), vec![Cmd::SetVolume(55)]);
    assert_eq!(
        m.snapshot.volume, 55,
        "the mixer shows the new value at once"
    );
    settle(&mut m);
    assert_eq!(update(&mut m, key('h')), vec![Cmd::SetVolume(50)]);
    m.snapshot.volume = 100;
    assert_eq!(update(&mut m, key('l')), vec![], "never above 100");
    m.snapshot.volume = 0;
    assert_eq!(update(&mut m, key('h')), vec![], "never below 0");

    m.mixer_row = MixerRow::Eq(2);
    assert_eq!(
        update(&mut m, key('l')),
        vec![Cmd::SetEq([0.0, 0.0, 1.0, 0.0, 0.0])]
    );
    assert_eq!(m.snapshot.eq, [0.0, 0.0, 1.0, 0.0, 0.0]);
    settle(&mut m);
    m.snapshot.eq = [0.0, 0.0, 12.0, 0.0, 0.0];
    assert_eq!(
        update(&mut m, key('l')),
        vec![],
        "clamped at +12 dB: nothing to send"
    );

    m.mixer_row = MixerRow::InputRate;
    assert_eq!(
        update(&mut m, key('l')),
        vec![Cmd::SetSampleRate {
            input_hz: Some(44100),
            output_hz: None
        }],
        "48000 is the last entry, so it wraps to 44100"
    );
    assert_eq!(
        m.snapshot.rates.as_ref().unwrap().input.sample_rate_hz,
        44100
    );
    m.mixer_row = MixerRow::OutputRate;
    settle(&mut m);
    assert_eq!(
        update(&mut m, key('l')),
        vec![Cmd::SetSampleRate {
            input_hz: None,
            output_hz: Some(48000)
        }]
    );
}

#[test]
fn first_write_sends_and_marks_in_flight() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    assert_eq!(update(&mut m, key('l')), vec![Cmd::SetVolume(55)]);
    assert_eq!(
        m.writes[MixerControl::Volume as usize],
        WriteSlot::InFlight { queued: None }
    );
    assert!(m.pending.is_none(), "mixer writes show no pending label");
}

#[test]
fn latest_value_wins() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    update(&mut m, key('l'));
    for _ in 0..3 {
        assert_eq!(update(&mut m, key('l')), vec![]);
    }
    assert_eq!(
        m.writes[MixerControl::Volume as usize],
        WriteSlot::InFlight {
            queued: Some(Cmd::SetVolume(70))
        }
    );
    assert_eq!(
        update(&mut m, written(MixerControl::Volume, Ok(()))),
        vec![Cmd::SetVolume(70)]
    );
    assert_eq!(
        m.writes[MixerControl::Volume as usize],
        WriteSlot::InFlight { queued: None }
    );
    assert_eq!(
        update(&mut m, written(MixerControl::Volume, Ok(()))),
        vec![]
    );
    assert_eq!(m.writes[MixerControl::Volume as usize], WriteSlot::Idle);
}

#[test]
fn failure_drops_the_queued_value() {
    let mut m = model_with_mock_snapshot();
    m.writes[MixerControl::Volume as usize] = WriteSlot::InFlight {
        queued: Some(Cmd::SetVolume(70)),
    };
    assert_eq!(
        update(&mut m, written(MixerControl::Volume, Err("x".into()))),
        vec![Cmd::Refresh]
    );
    assert_eq!(m.writes[MixerControl::Volume as usize], WriteSlot::Idle);
    assert_eq!(m.error.as_deref(), Some("x"));
}

#[test]
fn rate_success_reloads_rates() {
    let mut m = model_with_mock_snapshot();
    m.writes[MixerControl::InputRate as usize] = WriteSlot::InFlight { queued: None };
    assert_eq!(
        update(&mut m, written(MixerControl::InputRate, Ok(()))),
        vec![Cmd::LoadRates]
    );
    let mut fresh = m.snapshot.rates.clone().unwrap();
    fresh.output.supported_hz = vec![96000];
    update(&mut m, Msg::Rates(Ok(fresh.clone())));
    assert_eq!(m.snapshot.rates, Some(fresh));
}

#[test]
fn fine_steps() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    assert_eq!(update(&mut m, key('L')), vec![Cmd::SetVolume(51)]);
    settle(&mut m);
    assert_eq!(update(&mut m, key('H')), vec![Cmd::SetVolume(50)]);
    m.mixer_row = MixerRow::Eq(1);
    assert_eq!(
        update(&mut m, key('L')),
        vec![Cmd::SetEq([0.0, 0.5, 0.0, 0.0, 0.0])]
    );
    settle(&mut m);
    assert_eq!(
        update(&mut m, key('H')),
        vec![Cmd::SetEq([0.0, 0.0, 0.0, 0.0, 0.0])]
    );
}

#[test]
fn controls_are_independent() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    update(&mut m, key('l'));
    m.mixer_row = MixerRow::Eq(0);
    assert_eq!(
        update(&mut m, key('l')),
        vec![Cmd::SetEq([1.0, 0.0, 0.0, 0.0, 0.0])]
    );
}

#[test]
fn plus_and_minus_change_volume_from_any_pane() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(m.pane, Pane::Sources);
    assert_eq!(update(&mut m, key('+')), vec![Cmd::SetVolume(55)]);
    update(&mut m, key('-'));
    update(&mut m, key('='));
    update(&mut m, key('_'));
    assert_eq!(m.snapshot.volume, 50);
}

#[test]
fn repeated_presses_accumulate() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(update(&mut m, key('+')), vec![Cmd::SetVolume(55)]);
    assert_eq!(update(&mut m, key('+')), vec![]);
    assert_eq!(update(&mut m, key('+')), vec![]);
    assert_eq!(m.snapshot.volume, 65);
    m.pane = Pane::Mixer;
    m.mixer_row = MixerRow::Eq(4);
    update(&mut m, key('l'));
    assert_eq!(update(&mut m, key('l')), vec![]);
    assert_eq!(
        update(
            &mut m,
            Msg::Written {
                control: MixerControl::Eq,
                result: Ok(())
            }
        ),
        vec![Cmd::SetEq([0.0, 0.0, 0.0, 0.0, 2.0])]
    );
}

#[test]
fn a_failed_mixer_change_shows_the_error_and_resyncs() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, key('+'));
    assert_eq!(
        update(
            &mut m,
            Msg::Written {
                control: MixerControl::Volume,
                result: Err("Nothing is playing yet. Pick a speaker first.".into())
            }
        ),
        vec![Cmd::Refresh],
        "the core's real value replaces the optimistic one"
    );
    assert_eq!(
        m.error.as_deref(),
        Some("Nothing is playing yet. Pick a speaker first.")
    );
}

#[test]
fn zero_resets_the_focused_band() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    m.mixer_row = MixerRow::Eq(1);
    m.snapshot.eq = [0.0, 3.0, 0.0, 0.0, 0.0];
    assert_eq!(update(&mut m, key('0')), vec![Cmd::SetEq([0.0; 5])]);
    assert_eq!(m.snapshot.eq, [0.0; 5]);
}

#[test]
fn x_stops_the_live_output() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(update(&mut m, key('x')), vec![]);
    m.snapshot.active_output = Some(live_sonos());
    assert_eq!(update(&mut m, key('x')), vec![Cmd::DeactivateOutput]);
    assert_eq!(m.pending.as_ref().unwrap().label, "Stopping");
}

#[test]
fn cd_keys_only_act_with_a_disc() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(update(&mut m, key(' ')), vec![]);
    m.snapshot.cd = Some(disc(false));
    assert_eq!(
        update(&mut m, key(' ')),
        vec![Cmd::ControlCd {
            action: CdAction::Play,
            position_ms: None,
            track: None
        }]
    );
    m.snapshot.cd = Some(disc(true));
    assert_eq!(
        update(&mut m, key(' ')),
        vec![Cmd::ControlCd {
            action: CdAction::Pause,
            position_ms: None,
            track: None
        }]
    );
    assert_eq!(
        update(&mut m, key('>')),
        vec![Cmd::ControlCd {
            action: CdAction::Next,
            position_ms: None,
            track: None
        }]
    );
    assert_eq!(
        update(&mut m, key('<')),
        vec![Cmd::ControlCd {
            action: CdAction::Prev,
            position_ms: None,
            track: None
        }]
    );
    assert_eq!(
        update(&mut m, key('.')),
        vec![Cmd::ControlCd {
            action: CdAction::Next,
            position_ms: None,
            track: None
        }]
    );
    assert_eq!(
        update(&mut m, key(',')),
        vec![Cmd::ControlCd {
            action: CdAction::Prev,
            position_ms: None,
            track: None
        }]
    );
}

fn control(action: CdAction, position_ms: Option<u64>, track: Option<u8>) -> Vec<Cmd> {
    vec![Cmd::ControlCd {
        action,
        position_ms,
        track,
    }]
}

fn cd_event(present: bool) -> Msg {
    Msg::Ws(Incoming::Event(WsEvent::CdStateChanged {
        present,
        playing: false,
        track: 1,
        track_count: 12,
        title: None,
        album: None,
        position_ms: 0,
        duration_ms: 0,
    }))
}

#[test]
fn seek_keys_clamp() {
    let mut m = model_with_mock_snapshot();
    let mut cd = disc(true);
    cd.duration_ms = 300_000;
    cd.position_ms = 5_000;
    m.snapshot.cd = Some(cd.clone());
    assert_eq!(
        update(&mut m, key('[')),
        control(CdAction::Seek, Some(0), None)
    );
    cd.position_ms = 295_000;
    m.snapshot.cd = Some(cd);
    assert_eq!(
        update(&mut m, key(']')),
        control(CdAction::Seek, Some(300_000), None)
    );
    assert_eq!(seek_target(100_000, 300_000, 10_000), 110_000);
    assert_eq!(seek_target(100_000, 300_000, -10_000), 90_000);
}

#[test]
fn goto_prompt() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.cd = Some(disc(true));
    assert_eq!(update(&mut m, key('g')), vec![]);
    assert!(matches!(m.modal, Modal::TrackPrompt { .. }));
    update(&mut m, key('x'));
    assert!(matches!(&m.modal, Modal::TrackPrompt { input, .. } if input.is_empty()));
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(
        matches!(m.modal, Modal::TrackPrompt { .. }),
        "empty Enter does nothing"
    );
    update(&mut m, key('1'));
    update(&mut m, key('2'));
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        control(CdAction::Goto, None, Some(12))
    );
    assert_eq!(m.modal, Modal::None);

    update(&mut m, key('g'));
    update(&mut m, key('1'));
    update(&mut m, key('3'));
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(matches!(
        &m.modal,
        Modal::TrackPrompt { error: Some(e), .. } if e == "Track 13 is not on this disc"
    ));
    update(&mut m, code(KeyCode::Esc));
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn eject_confirms() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.cd = Some(disc(true));
    assert_eq!(update(&mut m, key('e')), vec![]);
    assert_eq!(m.modal, Modal::ConfirmEject);
    assert_eq!(
        update(&mut m, key('y')),
        control(CdAction::Eject, None, None)
    );
    assert_eq!(m.modal, Modal::None);
    update(&mut m, key('e'));
    assert_eq!(update(&mut m, key('n')), vec![]);
    assert_eq!(m.modal, Modal::None);
    update(&mut m, key('e'));
    assert_eq!(update(&mut m, code(KeyCode::Esc)), vec![]);
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn cd_keys_need_a_disc() {
    let mut m = model_with_mock_snapshot();
    for c in ['[', ']', 'g', 'e'] {
        assert_eq!(update(&mut m, key(c)), vec![]);
        assert_eq!(m.modal, Modal::None);
    }
}

#[test]
fn prompt_closes_when_the_disc_goes() {
    for open in ['g', 'e'] {
        let mut m = model_with_mock_snapshot();
        m.snapshot.cd = Some(disc(true));
        update(&mut m, key(open));
        assert_ne!(m.modal, Modal::None);
        assert_eq!(update(&mut m, cd_event(false)), vec![]);
        assert_eq!(m.modal, Modal::None);
    }
}

#[test]
fn m_toggles_the_monitor_and_locks_the_panes() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(update(&mut m, key('m')), vec![Cmd::SetService(false)]);
    update(
        &mut m,
        Msg::Ws(Incoming::Event(WsEvent::ServiceStateChanged {
            enabled: false,
        })),
    );
    assert!(m.locked());
    assert_eq!(update(&mut m, key('j')), vec![]);
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(update(&mut m, key('m')), vec![Cmd::SetService(true)]);
}

#[test]
fn i_toggles_status_even_when_locked() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.status.as_mut().unwrap().service_enabled = false;
    assert!(m.locked());
    assert_eq!(update(&mut m, key('i')), vec![]);
    assert_eq!(m.modal, Modal::Status);
    assert_eq!(update(&mut m, key('x')), vec![]);
    assert_eq!(m.modal, Modal::Status);
    update(&mut m, key('i'));
    assert_eq!(m.modal, Modal::None);
    update(&mut m, key('i'));
    update(&mut m, code(KeyCode::Esc));
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn core_down_blocks_core_commands() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, Msg::Snapshot(Err("down".into())));
    assert!(m.core_down);
    m.pane = Pane::Sources;
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(m.notice.as_deref(), Some("on-air-core is not reachable"));
    assert_eq!(m.pending, None);
    m.autostart = Some(false);
    assert_eq!(update(&mut m, key('a')), vec![Cmd::SetAutostart(true)]);
}

#[test]
fn core_down_still_allows_refresh_settings_and_quit() {
    let mut m = model_with_mock_snapshot();
    m.core_down = true;
    assert_eq!(update(&mut m, key('r')), vec![Cmd::Refresh]);
    assert_eq!(update(&mut m, key('b')), vec![Cmd::OpenBluetoothSettings]);
    assert_eq!(update(&mut m, key('q')), vec![Cmd::Quit]);
}

#[test]
fn firewall_repair_requires_confirmation_and_only_runs_on_the_host() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.status.as_mut().unwrap().firewall_warning = Some(crate::api::FirewallWarning {
        subnet: "192.168.5.0/24".into(),
        port: 47990,
        message: "UFW is blocking on-air audio from the local network.".into(),
        repair_command: "sudo ufw allow from 192.168.5.0/24 to any port 47990 proto tcp".into(),
    });
    assert!(update(&mut m, key('f')).is_empty());
    assert_eq!(m.modal, Modal::Firewall);
    assert!(update(&mut m, code(KeyCode::Esc)).is_empty());
    assert_eq!(m.modal, Modal::None);
    m.base = "http://192.168.5.101:47990".into();
    update(&mut m, key('f'));
    assert!(update(&mut m, code(KeyCode::Enter)).is_empty());
    m.base = "http://127.0.0.1:47990".into();
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::RepairFirewall]
    );
    assert!(
        update(&mut m, code(KeyCode::Enter)).is_empty(),
        "no duplicate prompts"
    );
    update(&mut m, done(LABEL_FIREWALL, Ok(())));
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn audio_check_recommends_without_applying_and_manual_selection_requires_a() {
    let mut m = model_with_mock_snapshot();
    assert!(update(&mut m, key('t')).is_empty());
    assert_eq!(m.modal, Modal::AudioCheck);
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::RunAudioCheck]
    );
    let report = crate::api::AudioBenchmark {
        input_hz: 44100,
        output_hz: 44100,
        duration_ms: 2200,
        pipeline_p95_ms: 10.0,
        scheduler_p95_ms: 2.0,
        worst_case_ms: 25.0,
        recommended_buffer_ms: 100,
        within_buffer_budget: true,
    };
    assert!(update(&mut m, Msg::Benchmark(Ok(report))).is_empty());
    assert_eq!(m.buffer_choice_ms, 100);
    assert_eq!(update(&mut m, key('a')), vec![Cmd::SetCaptureBuffer(100)]);
    update(&mut m, done(LABEL_BUFFER, Ok(())));
    assert!(update(&mut m, code(KeyCode::Right)).is_empty());
    assert_eq!(m.buffer_choice_ms, 200);
    assert_eq!(update(&mut m, key('a')), vec![Cmd::SetCaptureBuffer(200)]);
}

#[test]
fn arrows_adjust_volume_from_every_main_pane() {
    for pane in [Pane::Sources, Pane::Outputs, Pane::Mixer] {
        for (code_value, expected) in [(KeyCode::Up, 55), (KeyCode::Down, 45)] {
            let mut m = model_with_mock_snapshot();
            m.pane = pane;
            m.snapshot.volume = 50;
            assert_eq!(
                update(&mut m, code(code_value)),
                vec![Cmd::SetVolume(expected)]
            );
            assert_eq!(m.snapshot.volume, expected);
        }
    }
}

#[test]
fn snapshot_ok_clears_core_down() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, Msg::Snapshot(Err("down".into())));
    let snapshot = m.snapshot.clone();
    update(&mut m, Msg::Snapshot(Ok(Box::new(snapshot))));
    assert!(!m.core_down);
}

#[test]
fn a_toggles_autostart() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(update(&mut m, key('a')), vec![Cmd::SetAutostart(true)]);
    update(&mut m, Msg::Autostart(Ok(true)));
    assert_eq!(m.autostart, Some(true));
    assert_eq!(update(&mut m, key('a')), vec![Cmd::SetAutostart(false)]);
    m.autostart = None;
    assert_eq!(
        update(&mut m, key('a')),
        vec![],
        "unknown state: nothing to toggle"
    );
}

fn ev(e: WsEvent) -> Msg {
    Msg::Ws(Incoming::Event(e))
}

fn output_changed() -> Msg {
    ev(WsEvent::OutputStateChanged {
        transport: "sonos".into(),
        device_name: "Mock Sonos".into(),
        active: true,
    })
}

fn frames(m: &mut Model, n: usize) -> Vec<Cmd> {
    (0..n).flat_map(|_| update(m, Msg::Frame)).collect()
}

#[test]
fn events_debounce_into_one_refresh() {
    let mut m = model_with_mock_snapshot();
    for _ in 0..3 {
        assert_eq!(update(&mut m, output_changed()), vec![]);
    }
    assert_eq!(m.refresh_in, Some(6));
    assert_eq!(frames(&mut m, 5), vec![]);
    assert_eq!(update(&mut m, Msg::Frame), vec![Cmd::Refresh]);
    assert_eq!(m.refresh_in, None);
    assert_eq!(frames(&mut m, 5), vec![]);
}

#[test]
fn device_joined_adds_placeholder_now() {
    let mut m = model_with_mock_snapshot();
    update(
        &mut m,
        ev(WsEvent::DeviceJoined {
            transport: "sonos".into(),
            id: "uuid:new".into(),
            name: "New".into(),
        }),
    );
    let placeholder = |m: &Model| {
        m.dest_rows()
            .into_iter()
            .any(|row| matches!(row, DestRow::Entry(e) if e.name == "New" && e.placeholder))
    };
    assert!(placeholder(&m));
    let snapshot = m.snapshot.clone();
    update(&mut m, Msg::Snapshot(Ok(Box::new(snapshot))));
    assert!(m.placeholders.is_empty());
    assert!(!placeholder(&m));
}

#[test]
fn device_left_removes_row_now() {
    let mut m = model_with_mock_snapshot();
    update(
        &mut m,
        ev(WsEvent::DeviceLeft {
            transport: "sonos".into(),
            id: "uuid:mock-sonos".into(),
        }),
    );
    assert!(!m
        .dest_rows()
        .iter()
        .any(|row| matches!(row, DestRow::Entry(e) if e.id == "uuid:mock-sonos")));
    assert_eq!(m.refresh_in, Some(6));
}

#[test]
fn background_refresh_every_15s() {
    let mut m = model_with_mock_snapshot();
    m.connection = Connection::Reconnecting;
    assert_eq!(frames(&mut m, 300), vec![]);
    m.connection = Connection::Open;
    m.since_refresh = 0;
    assert_eq!(frames(&mut m, 299), vec![]);
    assert_eq!(update(&mut m, Msg::Frame), vec![Cmd::Refresh]);
    assert_eq!(m.since_refresh, 0);
}

#[test]
fn cd_and_monitor_events_unchanged() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(
            &mut m,
            ev(WsEvent::CdStateChanged {
                present: true,
                playing: true,
                track: 2,
                track_count: 9,
                title: None,
                album: Some("Album".into()),
                position_ms: 1,
                duration_ms: 2
            })
        ),
        vec![]
    );
    assert_eq!(m.snapshot.cd.as_ref().map(|cd| cd.track), Some(2));
    assert_eq!(m.refresh_in, None);
    assert_eq!(
        update(&mut m, ev(WsEvent::ServiceStateChanged { enabled: false })),
        vec![]
    );
    assert!(!m.service_on());
    assert_eq!(m.refresh_in, Some(6));
    assert_eq!(
        update(
            &mut m,
            ev(WsEvent::LevelMeter {
                rms: 0.1,
                peak: 0.2
            })
        ),
        vec![]
    );
    assert_eq!(update(&mut m, ev(WsEvent::Heartbeat)), vec![]);
}

#[test]
fn a_refused_sleep_inhibitor_becomes_a_notice_line() {
    let mut m = Model::new();
    let mut snap = model_with_mock_snapshot().snapshot;
    snap.status.as_mut().unwrap().sleep_inhibited = Some(false);
    update(&mut m, Msg::Snapshot(Ok(Box::new(snap.clone()))));
    assert!(
        m.notice
            .as_deref()
            .is_some_and(|n| n.contains("sleep not inhibited")),
        "{:?}",
        m.notice
    );
    snap.status.as_mut().unwrap().sleep_inhibited = Some(true);
    update(&mut m, Msg::Snapshot(Ok(Box::new(snap.clone()))));
    assert_eq!(m.notice, None);
    snap.status.as_mut().unwrap().sleep_inhibited = Some(false);
    snap.status.as_mut().unwrap().service_enabled = false;
    update(&mut m, Msg::Snapshot(Ok(Box::new(snap))));
    assert_eq!(
        m.notice, None,
        "nothing to inhibit while the monitor is off"
    );
}

#[test]
fn a_source_that_dies_says_so() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(
            &mut m,
            Msg::Ws(Incoming::Event(WsEvent::InputStateChanged {
                name: None,
                active: false,
                error: Some("parec exited".into()),
            }))
        ),
        vec![]
    );
    assert_eq!(m.refresh_in, Some(6));
    assert_eq!(
        m.error.as_deref(),
        Some("The source stopped unexpectedly. Choose it again to restart.")
    );
}

#[test]
fn unknown_ws_events_are_ignored() {
    let mut m = model_with_mock_snapshot();
    let before = m.clone();
    assert_eq!(
        update(&mut m, Msg::Ws(Incoming::Event(WsEvent::Unknown))),
        vec![]
    );
    assert_eq!(m, before);
}

#[test]
fn connection_changes_keep_the_snapshot() {
    let mut m = model_with_mock_snapshot();
    let snapshot = m.snapshot.clone();
    assert_eq!(
        update(
            &mut m,
            Msg::Ws(Incoming::Connection(Connection::Reconnecting))
        ),
        vec![]
    );
    assert_eq!(m.connection, Connection::Reconnecting);
    assert_eq!(m.snapshot, snapshot);
    assert_eq!(
        update(&mut m, Msg::Ws(Incoming::Connection(Connection::Open))),
        vec![Cmd::Refresh]
    );
}

#[test]
fn a_failed_snapshot_sets_the_error_and_keeps_the_old_one() {
    let mut m = model_with_mock_snapshot();
    let snapshot = m.snapshot.clone();
    update(&mut m, Msg::Snapshot(Err("Core unreachable".into())));
    assert_eq!(m.error.as_deref(), Some("Core unreachable"));
    assert_eq!(m.snapshot, snapshot);
}

#[test]
fn done_clears_pending_and_errors_show_once() {
    let mut m = model_with_mock_snapshot();
    m.pending = Some(Pending {
        label: "Switching source".into(),
    });
    assert_eq!(
        update(
            &mut m,
            Msg::Done {
                label: "Switching source".into(),
                result: Ok(())
            }
        ),
        vec![Cmd::Refresh]
    );
    assert!(m.pending.is_none());
    assert!(m.error.is_none());
    update(
        &mut m,
        Msg::Done {
            label: "Switching source".into(),
            result: Err("That source is gone.".into()),
        },
    );
    assert_eq!(m.error.as_deref(), Some("That source is gone."));
    update(&mut m, code(KeyCode::Esc));
    assert!(m.error.is_none());
}

#[test]
fn q_quits_and_help_toggles() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, key('?'));
    assert_eq!(m.modal, Modal::Help);
    update(&mut m, key('q'));
    assert_eq!(m.modal, Modal::None, "the first key closes help");
    assert!(!m.quit);
    assert_eq!(update(&mut m, key('q')), vec![Cmd::Quit]);
    assert!(m.quit);
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(
            &mut m,
            Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        ),
        vec![Cmd::Quit]
    );
}

#[test]
fn resize_and_accent_are_recorded() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, Msg::Resize(120, 40));
    assert_eq!(m.size, (120, 40));
    update(&mut m, Msg::Accent(Color::Rgb(1, 2, 3)));
    assert_eq!(m.accent, Color::Rgb(1, 2, 3));
}

fn live_model() -> Model {
    let mut m = model_with_mock_snapshot();
    m.snapshot.active_output = Some(ActiveOutputView {
        error: None,
        output: ActiveOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into(),
            device_name: "Mock Sonos".into(),
        },
        state: Some(OutputPhase::Live),
    });
    m
}

fn level(m: &mut Model, rms: f32, peak: f32) {
    update(
        m,
        Msg::Ws(Incoming::Event(WsEvent::LevelMeter { rms, peak })),
    );
}

#[test]
fn level_event_sets_meter() {
    let mut m = live_model();
    level(&mut m, 0.5, 1.0);
    assert!((m.meter.rms_db + 6.02).abs() < 0.01, "{:?}", m.meter);
    assert_eq!(m.meter.peak_db, 0.0);
    assert_eq!(m.meter.hold, 10);
    assert_eq!(m.meter.quiet, 0);
}

#[test]
fn level_db_clamps_to_the_scale() {
    assert_eq!(level_db(0.0), -60.0);
    assert_eq!(level_db(1e-9), -60.0);
    assert_eq!(level_db(2.0), 0.0);
}

#[test]
fn peak_holds_then_falls() {
    let mut m = live_model();
    level(&mut m, 0.5, 1.0);
    for _ in 0..10 {
        update(&mut m, Msg::Frame);
    }
    assert_eq!(m.meter.peak_db, 0.0);
    update(&mut m, Msg::Frame);
    assert_eq!(m.meter.peak_db, -1.5);
    update(&mut m, Msg::Frame);
    assert_eq!(m.meter.peak_db, -3.0);
    // A lower peak does not disturb the falling one; a higher one resets the hold.
    level(&mut m, 0.5, 0.001);
    assert_eq!(m.meter.peak_db, -3.0);
    level(&mut m, 0.5, 1.0);
    assert_eq!((m.meter.peak_db, m.meter.hold), (0.0, 10));
}

#[test]
fn meter_times_out() {
    let mut m = live_model();
    level(&mut m, 0.5, 1.0);
    for _ in 0..20 {
        update(&mut m, Msg::Frame);
    }
    assert_eq!((m.meter.rms_db, m.meter.peak_db), (-60.0, -60.0));
}

#[test]
fn meter_reads_zero_when_monitor_off_or_idle() {
    let mut off = live_model();
    off.snapshot.status.as_mut().unwrap().service_enabled = false;
    level(&mut off, 0.5, 1.0);
    assert_eq!((off.meter.rms_db, off.meter.peak_db), (-60.0, -60.0));

    let mut idle = live_model();
    idle.snapshot.active_output = None;
    level(&mut idle, 0.5, 1.0);
    assert_eq!((idle.meter.rms_db, idle.meter.peak_db), (-60.0, -60.0));

    let mut failed = live_model();
    failed.snapshot.active_output.as_mut().unwrap().state = Some(OutputPhase::Failed);
    level(&mut failed, 0.5, 1.0);
    assert_eq!(failed.meter.peak_db, -60.0);
}

fn mouse(target: Target, rect: Rect, column: u16, row: u16, kind: MouseKind) -> Msg {
    Msg::Mouse {
        target,
        rect,
        column,
        row,
        kind,
    }
}

fn click(target: Target) -> Msg {
    mouse(target, Rect::new(0, 0, 10, 1), 0, 0, MouseKind::Down)
}

fn wheel(target: Target, kind: MouseKind) -> Msg {
    mouse(target, Rect::new(0, 0, 10, 1), 0, 0, kind)
}

#[test]
fn click_source_row_selects_and_activates() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.inputs = vec!["Mock Monitor".into(), "Built-in mic".into()];
    m.pane = Pane::Mixer;
    assert_eq!(
        update(&mut m, click(Target::SourceRow(1))),
        vec![Cmd::ActivateInput("Built-in mic".into())]
    );
    assert_eq!(m.pane, Pane::Sources);
    assert_eq!(m.source_index, 1);
    assert_eq!(m.pending.as_ref().unwrap().label, LABEL_SOURCE);
}

#[test]
fn click_dest_row_runs_enter_flow() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.bluetooth = Some(vec![
        bt("bt-speaker", true, true),
        bt("bt-unpaired", false, false),
    ]);
    let index = |m: &Model, id: &str| {
        m.dest_rows()
            .iter()
            .position(|row| matches!(row, DestRow::Entry(e) if e.id == id))
            .unwrap()
    };
    let sonos = index(&m, "uuid:mock-sonos");
    assert_eq!(
        update(&mut m, click(Target::DestRow(sonos))),
        vec![Cmd::ActivateOutput {
            transport: "sonos".into(),
            device_id: "uuid:mock-sonos".into()
        }]
    );
    assert_eq!(m.pane, Pane::Outputs);
    assert_eq!(m.output_index, sonos);
    assert_eq!(step(&m), Some(ActivationStep::Activating));

    let unpaired = index(&m, "bt-unpaired");
    assert_eq!(update(&mut m, click(Target::DestRow(unpaired))), vec![]);
    assert_eq!(
        m.notice.as_deref(),
        Some("Already connecting Mock Sonos…"),
        "the busy guard applies to clicks too"
    );

    let mut m = model_with_mock_snapshot();
    m.snapshot.bluetooth = Some(vec![bt("bt-unpaired", false, false)]);
    let unpaired = index(&m, "bt-unpaired");
    assert_eq!(update(&mut m, click(Target::DestRow(unpaired))), vec![]);
    assert!(matches!(m.modal, Modal::Pin { .. }));
}

#[test]
fn horizontal_volume_clicks_and_drags_use_the_displayed_percentage_scale() {
    let rect = Rect::new(10, 5, 21, 1);
    for (column, kind, value) in [
        (10, MouseKind::Down, 0),
        (20, MouseKind::Down, 50),
        (30, MouseKind::Down, 100),
        (0, MouseKind::Drag, 0),
        (500, MouseKind::Drag, 100),
    ] {
        let mut m = model_with_mock_snapshot();
        m.snapshot.volume = 37;
        assert_eq!(
            update(&mut m, mouse(Target::VolumeSlider, rect, column, 5, kind)),
            vec![Cmd::SetVolume(value)]
        );
        assert_eq!((m.pane, m.mixer_row), (Pane::Mixer, MixerRow::Volume));
        assert_eq!(m.snapshot.volume, value);
    }
}

#[test]
fn drag_on_eq_fader_sets_value_through_write() {
    let mut m = model_with_mock_snapshot();
    let rect = Rect::new(30, 8, 3, 5);
    let fader = Target::Fader(MixerRow::Eq(2));
    assert_eq!(
        update(&mut m, mouse(fader, rect, 31, rect.y, MouseKind::Down)),
        vec![],
        "the first click focuses"
    );
    assert_eq!(
        update(&mut m, mouse(fader, rect, 31, rect.y, MouseKind::Down)),
        vec![Cmd::SetEq([0.0, 0.0, 12.0, 0.0, 0.0])]
    );
    assert_eq!(m.snapshot.eq[2], 12.0);
    assert_eq!((m.pane, m.mixer_row), (Pane::Mixer, MixerRow::Eq(2)));
    assert_eq!(
        update(&mut m, mouse(fader, rect, 31, rect.y + 4, MouseKind::Drag)),
        vec![]
    );
    assert_eq!(m.snapshot.eq[2], -12.0);
    assert_eq!(
        m.writes[MixerControl::Eq as usize],
        WriteSlot::InFlight {
            queued: Some(Cmd::SetEq([0.0, 0.0, -12.0, 0.0, 0.0]))
        }
    );

    let volume = Target::Fader(MixerRow::Volume);
    update(&mut m, mouse(volume, rect, 31, rect.y, MouseKind::Down));
    assert_eq!(m.snapshot.volume, 50);
    assert_eq!(
        update(&mut m, mouse(volume, rect, 31, rect.y + 2, MouseKind::Down)),
        vec![],
        "the middle cell is already 50: nothing to send"
    );
    assert_eq!(
        update(&mut m, mouse(volume, rect, 31, rect.y + 1, MouseKind::Down)),
        vec![Cmd::SetVolume(75)]
    );
}

#[test]
fn a_one_row_fader_click_only_focuses_it() {
    let mut m = model_with_mock_snapshot();
    let fader = Target::Fader(MixerRow::Eq(1));
    assert_eq!(update(&mut m, click(fader)), vec![]);
    assert_eq!((m.pane, m.mixer_row), (Pane::Mixer, MixerRow::Eq(1)));
    assert_eq!(m.snapshot.eq, [0.0; 5]);
}

#[test]
fn wheel_on_fader_fine_steps() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(
            &mut m,
            wheel(Target::Fader(MixerRow::Volume), MouseKind::ScrollUp)
        ),
        vec![Cmd::SetVolume(51)]
    );
    settle(&mut m);
    assert_eq!(
        update(
            &mut m,
            wheel(Target::Fader(MixerRow::Eq(0)), MouseKind::ScrollDown)
        ),
        vec![Cmd::SetEq([-0.5, 0.0, 0.0, 0.0, 0.0])]
    );
}

#[test]
fn wheel_on_list_moves_cursor() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.inputs = vec!["Mock Monitor".into(), "Built-in mic".into()];
    m.pane = Pane::Mixer;
    assert_eq!(
        update(&mut m, wheel(Target::SourceRow(0), MouseKind::ScrollDown)),
        vec![]
    );
    assert_eq!((m.pane, m.source_index), (Pane::Sources, 1));
    update(&mut m, wheel(Target::SourceRow(1), MouseKind::ScrollUp));
    assert_eq!(m.source_index, 0);

    let before = m.output_index;
    update(
        &mut m,
        wheel(Target::DestRow(before), MouseKind::ScrollDown),
    );
    assert_eq!(m.pane, Pane::Outputs);
    assert!(m.output_index > before, "{} > {before}", m.output_index);
}

#[test]
fn drag_over_a_list_row_does_nothing() {
    let mut m = model_with_mock_snapshot();
    m.pane = Pane::Mixer;
    let msg = mouse(
        Target::SourceRow(0),
        Rect::new(0, 0, 10, 1),
        0,
        0,
        MouseKind::Drag,
    );
    assert_eq!(update(&mut m, msg), vec![]);
    assert_eq!(m.pane, Pane::Mixer, "focus stays put");
}

#[test]
fn click_rate_cycles() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(&mut m, click(Target::Rate(MixerRow::InputRate))),
        vec![Cmd::SetSampleRate {
            input_hz: Some(44100),
            output_hz: None
        }]
    );
    assert_eq!(
        m.writes[MixerControl::InputRate as usize],
        WriteSlot::InFlight { queued: None }
    );
    assert_eq!(
        update(
            &mut m,
            wheel(Target::Rate(MixerRow::OutputRate), MouseKind::ScrollUp)
        ),
        vec![Cmd::SetSampleRate {
            input_hz: None,
            output_hz: Some(48000)
        }]
    );
    settle(&mut m);
    assert_eq!(
        update(
            &mut m,
            wheel(Target::Rate(MixerRow::OutputRate), MouseKind::ScrollDown)
        ),
        vec![Cmd::SetSampleRate {
            input_hz: None,
            output_hz: Some(44100)
        }]
    );
}

#[test]
fn click_cd_controls() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.cd = Some(disc(false));
    assert_eq!(
        update(&mut m, click(Target::CdButton(CdButton::PlayPause))),
        control(CdAction::Play, None, None)
    );
    assert_eq!(
        update(&mut m, click(Target::CdButton(CdButton::Next))),
        control(CdAction::Next, None, None)
    );
    assert_eq!(
        update(&mut m, click(Target::CdButton(CdButton::Prev))),
        control(CdAction::Prev, None, None)
    );
    assert_eq!(
        update(&mut m, click(Target::CdButton(CdButton::Eject))),
        vec![]
    );
    assert_eq!(m.modal, Modal::ConfirmEject);
    m.modal = Modal::None;

    let rect = Rect::new(20, 1, 40, 1);
    let cmds = update(
        &mut m,
        mouse(
            Target::CdProgress,
            rect,
            rect.x + rect.width / 2,
            1,
            MouseKind::Down,
        ),
    );
    let duration = 270_000;
    let column = duration / u64::from(rect.width);
    match cmds.as_slice() {
        [Cmd::ControlCd {
            action: CdAction::Seek,
            position_ms: Some(position),
            track: None,
        }] => assert!(
            position.abs_diff(duration / 2) <= column,
            "{position} is not near {}",
            duration / 2
        ),
        other => panic!("{other:?}"),
    }

    m.snapshot.cd = None;
    assert_eq!(update(&mut m, click(Target::CdProgress)), vec![]);
    assert_eq!(
        update(&mut m, click(Target::CdButton(CdButton::Eject))),
        vec![]
    );
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn click_header_tags() {
    let mut m = model_with_mock_snapshot();
    assert_eq!(
        update(&mut m, click(Target::MonitorTag)),
        vec![Cmd::SetService(false)]
    );
    assert_eq!(m.pending.as_ref().unwrap().label, LABEL_MONITOR);
    assert_eq!(update(&mut m, click(Target::StatusTag)), vec![]);
    assert_eq!(m.modal, Modal::Status);
}

#[test]
fn click_outside_overlay_closes_it() {
    let mut m = model_with_mock_snapshot();
    m.modal = Modal::Help;
    assert_eq!(update(&mut m, click(Target::Overlay)), vec![]);
    assert_eq!(m.modal, Modal::Help, "a click inside keeps it open");
    assert_eq!(
        update(&mut m, wheel(Target::OverlayOutside, MouseKind::ScrollDown)),
        vec![]
    );
    assert_eq!(m.modal, Modal::Help, "the wheel does not close it");
    assert_eq!(update(&mut m, click(Target::OverlayOutside)), vec![]);
    assert_eq!(m.modal, Modal::None);

    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    assert!(matches!(m.modal, Modal::Pin { .. }));
    update(&mut m, click(Target::OverlayOutside));
    assert_eq!(m.modal, Modal::None);
    assert_eq!(m.activation, None, "same as Esc");
}

#[test]
fn mouse_respects_lock_and_core_down() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.status.as_mut().unwrap().service_enabled = false;
    m.snapshot.cd = Some(disc(false));
    assert!(m.locked());
    for target in [
        Target::SourceRow(0),
        Target::DestRow(1),
        Target::Fader(MixerRow::Volume),
        Target::Rate(MixerRow::InputRate),
        Target::CdButton(CdButton::PlayPause),
        Target::CdProgress,
    ] {
        assert_eq!(update(&mut m, click(target)), vec![], "{target:?}");
        assert_eq!(
            update(&mut m, wheel(target, MouseKind::ScrollUp)),
            vec![],
            "{target:?}"
        );
    }
    assert_eq!(m.pane, Pane::Sources);
    assert_eq!(m.snapshot.volume, 50);
    assert_eq!(update(&mut m, click(Target::StatusTag)), vec![]);
    assert_eq!(m.modal, Modal::Status);
    update(&mut m, click(Target::OverlayOutside));
    assert_eq!(m.modal, Modal::None);
    assert_eq!(
        update(&mut m, click(Target::MonitorTag)),
        vec![Cmd::SetService(true)]
    );

    let mut m = model_with_mock_snapshot();
    m.core_down = true;
    m.pane = Pane::Mixer;
    assert_eq!(update(&mut m, click(Target::SourceRow(0))), vec![]);
    assert_eq!(m.notice.as_deref(), Some(CORE_DOWN_COPY));
    assert_eq!(m.pane, Pane::Mixer, "the click is undone");
    assert_eq!(
        update(
            &mut m,
            wheel(Target::Fader(MixerRow::Volume), MouseKind::ScrollUp)
        ),
        vec![]
    );
    assert_eq!(m.snapshot.volume, 50);
    assert_eq!(update(&mut m, click(Target::StatusTag)), vec![]);
    assert_eq!(m.modal, Modal::Status);
}

#[test]
fn enter_in_the_pin_modal_while_pairing_sends_nothing() {
    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairBluetooth("bt-unpaired".into())]
    );
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(step(&m), Some(ActivationStep::Pairing));
    assert_eq!(
        update(&mut m, done(LABEL_PAIR, Ok(()))),
        vec![Cmd::ConnectBluetooth("bt-unpaired".into())]
    );
    assert_eq!(step(&m), Some(ActivationStep::Connecting));
}

#[test]
fn enter_after_a_failed_pairing_tries_again() {
    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, done(LABEL_PAIR, Err("no luck".into())));
    assert!(matches!(m.modal, Modal::Pin { .. }));
    assert_eq!(
        update(&mut m, code(KeyCode::Enter)),
        vec![Cmd::PairBluetooth("bt-unpaired".into())]
    );
}

fn airplay_pin_modal(m: &mut Model, input: &str) {
    m.modal = Modal::Pin {
        target: PairTarget::Airplay {
            device_id: "ap-new".into(),
            name: "HomePod".into(),
        },
        input: input.into(),
        error: None,
    };
}

#[test]
fn a_refresh_error_does_not_land_in_the_pin_modal() {
    let mut m = model_with_mock_snapshot();
    airplay_pin_modal(&mut m, "12");
    assert_eq!(
        update(&mut m, done(LABEL_REFRESH, Err("partial".into()))),
        vec![]
    );
    assert_eq!(m.error.as_deref(), Some("partial"));
    assert!(
        matches!(&m.modal, Modal::Pin { input, error: None, .. } if input == "12"),
        "{:?}",
        m.modal
    );
}

#[test]
fn a_pairing_reply_after_esc_activates_nothing() {
    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Esc));
    assert_eq!(update(&mut m, done(LABEL_PAIR, Ok(()))), vec![Cmd::Refresh]);
    assert_eq!(m.activation, None);
    assert_eq!(m.pending, None);
}

#[test]
fn a_stale_pairing_reply_leaves_another_devices_modal_alone() {
    let mut m = on_dest("bt-unpaired");
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Enter));
    update(&mut m, code(KeyCode::Esc));
    airplay_pin_modal(&mut m, "12");
    assert_eq!(update(&mut m, done(LABEL_PAIR, Ok(()))), vec![Cmd::Refresh]);
    assert!(matches!(&m.modal, Modal::Pin { input, .. } if input == "12"));
    assert_eq!(m.activation, None);
    assert_eq!(
        update(&mut m, done(LABEL_PAIR, Err("late".into()))),
        vec![Cmd::Refresh]
    );
    assert!(matches!(&m.modal, Modal::Pin { error: None, .. }));
}

#[test]
fn a_reply_for_another_step_does_not_advance_the_chain() {
    let mut m = on_dest("uuid:mock-sonos");
    update(&mut m, code(KeyCode::Enter));
    assert_eq!(step(&m), Some(ActivationStep::Activating));
    assert_eq!(
        update(&mut m, done(LABEL_CONNECT, Ok(()))),
        vec![Cmd::Refresh]
    );
    assert_eq!(step(&m), Some(ActivationStep::Activating));
    assert_eq!(
        m.pending.as_ref().map(|p| p.label.as_str()),
        Some(LABEL_OUTPUT),
        "only a matching label clears the pending one"
    );
}

#[test]
fn losing_the_socket_probes_the_core_once() {
    let mut m = model_with_mock_snapshot();
    m.connection = Connection::Open;
    assert_eq!(
        update(
            &mut m,
            Msg::Ws(Incoming::Connection(Connection::Reconnecting))
        ),
        vec![Cmd::Refresh]
    );
    assert_eq!(
        update(
            &mut m,
            Msg::Ws(Incoming::Connection(Connection::Reconnecting))
        ),
        vec![],
        "only the drop from Open probes"
    );
    m.connection = Connection::Open;
    assert_eq!(
        update(&mut m, Msg::Ws(Incoming::Connection(Connection::Closed))),
        vec![Cmd::Refresh]
    );
    update(&mut m, Msg::Snapshot(Err("Core unreachable".into())));
    assert!(m.core_down);
}

#[test]
fn a_click_on_an_unfocused_fader_only_focuses_it() {
    let mut m = model_with_mock_snapshot();
    let rect = Rect::new(30, 8, 5, 5);
    let volume = Target::Fader(MixerRow::Volume);
    assert_eq!(
        update(&mut m, mouse(volume, rect, 31, rect.y, MouseKind::Down)),
        vec![]
    );
    assert_eq!((m.pane, m.mixer_row), (Pane::Mixer, MixerRow::Volume));
    assert_eq!(m.snapshot.volume, 50);
    assert_eq!(
        update(&mut m, mouse(volume, rect, 31, rect.y, MouseKind::Down)),
        vec![Cmd::SetVolume(100)]
    );

    let band = Target::Fader(MixerRow::Eq(1));
    assert_eq!(
        update(&mut m, mouse(band, rect, 31, rect.y, MouseKind::Down)),
        vec![],
        "another fader is unfocused again"
    );
    assert_eq!(m.mixer_row, MixerRow::Eq(1));
    assert_eq!(m.snapshot.eq, [0.0; 5]);
}

#[test]
fn the_destination_cursor_follows_its_device() {
    let mut m = on_dest("ap-living");
    let id = |m: &Model| m.selected_dest().map(|e| e.id);
    update(
        &mut m,
        ev(WsEvent::DeviceJoined {
            transport: "sonos".into(),
            id: "uuid:new".into(),
            name: "New".into(),
        }),
    );
    assert_eq!(id(&m).as_deref(), Some("ap-living"));

    let mut snapshot = m.snapshot.clone();
    snapshot
        .outputs
        .insert(0, output("uuid:other", "Other", "sonos", false, true));
    update(&mut m, Msg::Snapshot(Ok(Box::new(snapshot))));
    assert_eq!(id(&m).as_deref(), Some("ap-living"));

    update(
        &mut m,
        ev(WsEvent::DeviceLeft {
            transport: "sonos".into(),
            id: "uuid:other".into(),
        }),
    );
    assert_eq!(id(&m).as_deref(), Some("ap-living"));

    update(
        &mut m,
        ev(WsEvent::DeviceLeft {
            transport: "airplay".into(),
            id: "ap-living".into(),
        }),
    );
    assert!(id(&m).is_some(), "falls back to a device row");
}

#[test]
fn esc_also_dismisses_the_notice() {
    let mut m = model_with_mock_snapshot();
    m.notice = Some(CORE_DOWN_COPY.into());
    m.error = Some("boom".into());
    update(&mut m, code(KeyCode::Esc));
    assert_eq!((m.notice.as_deref(), m.error.as_deref()), (None, None));
}

#[test]
fn q_quits_from_the_status_overlay() {
    let mut m = model_with_mock_snapshot();
    update(&mut m, key('i'));
    assert_eq!(m.modal, Modal::Status);
    assert_eq!(update(&mut m, key('q')), vec![Cmd::Quit]);
    assert!(m.quit);
}

#[test]
fn a_three_digit_track_past_255_is_not_on_the_disc() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.cd = Some(disc(true));
    update(&mut m, key('g'));
    for c in "300".chars() {
        update(&mut m, key(c));
    }
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert!(
        matches!(
            &m.modal,
            Modal::TrackPrompt { error: Some(e), .. } if e == "Track 300 is not on this disc"
        ),
        "{:?}",
        m.modal
    );
}

#[test]
fn a_mixer_write_leaves_other_errors_alone() {
    let mut m = model_with_mock_snapshot();
    m.error = Some("The speaker did not answer.".into());
    update(&mut m, key('+'));
    update(&mut m, written(MixerControl::Volume, Ok(())));
    assert_eq!(m.error.as_deref(), Some("The speaker did not answer."));
}

#[test]
fn a_snapshot_keeps_values_still_being_written() {
    let mut m = model_with_mock_snapshot();
    let stale = m.snapshot.clone();
    m.pane = Pane::Mixer;
    update(&mut m, key('+'));
    m.mixer_row = MixerRow::Eq(0);
    update(&mut m, key('l'));
    m.mixer_row = MixerRow::InputRate;
    update(&mut m, key('l'));
    m.mixer_row = MixerRow::OutputRate;
    update(&mut m, key('l'));
    let optimistic = m.snapshot.clone();
    update(&mut m, Msg::Snapshot(Ok(Box::new(stale.clone()))));
    assert_eq!(m.snapshot.volume, optimistic.volume);
    assert_eq!(m.snapshot.eq, optimistic.eq);
    assert_eq!(m.snapshot.rates, optimistic.rates);

    settle(&mut m);
    update(&mut m, Msg::Snapshot(Ok(Box::new(stale.clone()))));
    assert_eq!(m.snapshot, stale, "settled controls take the core's values");
}

#[test]
fn a_placeholder_row_cannot_be_activated() {
    let mut m = model_with_mock_snapshot();
    m.snapshot.bluetooth = Some(vec![]);
    update(
        &mut m,
        ev(WsEvent::DeviceJoined {
            transport: "sonos".into(),
            id: "uuid:new".into(),
            name: "New".into(),
        }),
    );
    let index = m
        .dest_rows()
        .iter()
        .position(|row| matches!(row, DestRow::Entry(e) if e.placeholder))
        .unwrap();
    assert_eq!(update(&mut m, click(Target::DestRow(index))), vec![]);
    assert_eq!(m.output_index, index);
    assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
    assert_eq!(m.activation, None);
    assert_eq!(m.modal, Modal::None);
}

#[test]
fn restart_works_while_the_core_is_down_and_ignores_an_old_switch_reply() {
    let mut model = live_model();
    model.base = crate::client::DEFAULT_BASE.into();
    model.core_down = true;
    model.modal = Modal::Status;
    assert_eq!(update(&mut model, key('R')), vec![Cmd::RestartService]);
    assert!(model.snapshot.active_output.is_none());
    assert_eq!(model.pending.as_ref().unwrap().label, LABEL_RESTART);
    assert!(update(&mut model, key('R')).is_empty());
    assert!(update(
        &mut model,
        Msg::Done {
            label: LABEL_OUTPUT.into(),
            result: Ok(())
        }
    )
    .is_empty());
    assert_eq!(model.pending.as_ref().unwrap().label, LABEL_RESTART);
    assert_eq!(
        update(
            &mut model,
            Msg::Done {
                label: LABEL_RESTART.into(),
                result: Ok(())
            }
        ),
        vec![Cmd::Refresh]
    );
    assert!(model.pending.is_none());
}

#[test]
fn restart_never_restarts_a_local_service_when_controlling_a_remote_core() {
    let mut model = live_model();
    model.base = "http://192.168.1.40:47990".into();
    assert!(update(&mut model, key('R')).is_empty());
    assert!(model.error.as_ref().unwrap().contains("remote host"));
}

#[test]
fn a_failed_output_snapshot_explains_the_receiver_failure() {
    let mut model = live_model();
    let mut snapshot = model.snapshot.clone();
    let active = snapshot.active_output.as_mut().unwrap();
    active.state = Some(OutputPhase::Failed);
    active.error = Some("AirPlay playback failed: receiver disconnected".into());
    update(&mut model, Msg::Snapshot(Ok(Box::new(snapshot))));
    assert_eq!(
        model.error.as_deref(),
        Some("AirPlay playback failed: receiver disconnected")
    );
}
