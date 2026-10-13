//! The pure half of the UI: `update(model, msg) -> commands`. No I/O lives
//! here, so every key, event and reply can be tested without a core.

use crate::api::{CdAction, WsEvent};
use crate::client::{Connection, Incoming};
use crate::keys::{action_for, Action};
use crate::model::{MixerRow, Modal, Model, OutputRow, PairTarget, Pane, Pending, Snapshot};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;

/// Everything that can happen to the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    Ws(Incoming),
    Snapshot(Result<Box<Snapshot>, String>),
    Done {
        label: String,
        result: Result<(), String>,
    },
    Autostart(Result<bool, String>),
    Accent(Color),
}

/// Everything the model asks the outside world to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Refresh,
    ActivateInput(String),
    ActivateOutput {
        transport: String,
        device_id: String,
    },
    DeactivateOutput,
    SetVolume(u8),
    SetEq([f32; 5]),
    SetSampleRate {
        input_hz: Option<u32>,
        output_hz: Option<u32>,
    },
    ControlCd(CdAction),
    PairAirplay {
        device_id: String,
        pin: String,
    },
    PairBluetooth(String),
    ConnectBluetooth(String),
    OpenBluetoothSettings,
    SetService(bool),
    QueryAutostart,
    SetAutostart(bool),
    Quit,
}

pub const VOLUME_STEP: u8 = 5;
pub const EQ_STEP_DB: f32 = 1.0;
pub const EQ_RANGE_DB: (f32, f32) = (-12.0, 12.0);

pub const LABEL_SOURCE: &str = "Switching source";
pub const LABEL_OUTPUT: &str = "Switching output";
pub const LABEL_STOP: &str = "Stopping";
pub const LABEL_PAIR: &str = "Pairing";
pub const LABEL_CONNECT: &str = "Connecting";
pub const LABEL_SERVICE: &str = "Changing service";
pub const LABEL_REFRESH: &str = "Refreshing";
pub const LABEL_AUTOSTART: &str = "Changing autostart";
pub const LABEL_SETTINGS: &str = "Opening Bluetooth settings";
pub const LABEL_MIXER: &str = "Adjusting";
pub const LABEL_CD: &str = "Disc";

pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    match msg {
        Msg::Key(key) => on_key(model, key),
        Msg::Resize(w, h) => {
            model.size = (w, h);
            vec![]
        }
        Msg::Tick => vec![],
        Msg::Ws(Incoming::Connection(connection)) => {
            model.connection = connection;
            if connection == Connection::Open {
                vec![Cmd::Refresh]
            } else {
                vec![]
            }
        }
        Msg::Ws(Incoming::Event(event)) => on_event(model, event),
        Msg::Snapshot(Ok(snapshot)) => {
            model.snapshot = *snapshot;
            if matches!(
                model.pending,
                Some(Pending { ref label }) if label == LABEL_REFRESH
            ) {
                model.pending = None;
            }
            clamp_selection(model);
            vec![]
        }
        Msg::Snapshot(Err(copy)) => {
            model.error = Some(copy);
            if matches!(
                model.pending,
                Some(Pending { ref label }) if label == LABEL_REFRESH
            ) {
                model.pending = None;
            }
            vec![]
        }
        Msg::Done { label, result } => on_done(model, &label, result),
        Msg::Autostart(Ok(enabled)) => {
            model.autostart = Some(enabled);
            if matches!(
                model.pending,
                Some(Pending { ref label }) if label == LABEL_AUTOSTART
            ) {
                model.pending = None;
            }
            vec![]
        }
        Msg::Autostart(Err(copy)) => {
            model.error = Some(copy);
            if matches!(
                model.pending,
                Some(Pending { ref label }) if label == LABEL_AUTOSTART
            ) {
                model.pending = None;
            }
            vec![]
        }
        Msg::Accent(color) => {
            model.accent = color;
            vec![]
        }
    }
}

fn on_done(model: &mut Model, label: &str, result: Result<(), String>) -> Vec<Cmd> {
    model.pending = None;
    match result {
        Ok(()) => {
            model.error = None;
            if label == LABEL_PAIR {
                if let Modal::Pin { target, .. } = std::mem::replace(&mut model.modal, Modal::None)
                {
                    let (transport, device_id) = match target {
                        PairTarget::Airplay { device_id, .. } => ("airplay", device_id),
                        PairTarget::Bluetooth { id, .. } => ("bluetooth", id),
                    };
                    model.pending = Some(Pending {
                        label: LABEL_OUTPUT.into(),
                    });
                    return vec![Cmd::ActivateOutput {
                        transport: transport.into(),
                        device_id,
                    }];
                }
            }
            if label == LABEL_SETTINGS || label == LABEL_MIXER {
                vec![]
            } else {
                vec![Cmd::Refresh]
            }
        }
        Err(copy) => {
            if let Modal::Pin { error, input, .. } = &mut model.modal {
                *error = Some(copy);
                input.clear();
            } else {
                model.error = Some(copy);
            }
            vec![]
        }
    }
}

fn on_event(model: &mut Model, event: WsEvent) -> Vec<Cmd> {
    match event {
        WsEvent::ServiceStateChanged { enabled } => {
            if let Some(status) = &mut model.snapshot.status {
                status.service_enabled = enabled;
            }
            vec![Cmd::Refresh]
        }
        WsEvent::CdStateChanged {
            present,
            playing,
            track,
            track_count,
            title,
            album,
            position_ms,
            duration_ms,
        } => {
            let tracks = model
                .snapshot
                .cd
                .take()
                .map(|cd| cd.tracks)
                .unwrap_or_default();
            model.snapshot.cd = Some(crate::api::CdStatus {
                present,
                playing,
                track,
                track_count,
                title,
                album,
                position_ms,
                duration_ms,
                tracks,
            });
            vec![]
        }
        WsEvent::OutputStateChanged { .. }
        | WsEvent::InputStateChanged { .. }
        | WsEvent::DeviceJoined { .. }
        | WsEvent::DeviceLeft { .. } => vec![Cmd::Refresh],
        WsEvent::LevelMeter { .. } | WsEvent::Heartbeat | WsEvent::Unknown => vec![],
    }
}

fn on_key(model: &mut Model, key: KeyEvent) -> Vec<Cmd> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        model.quit = true;
        return vec![Cmd::Quit];
    }
    match &mut model.modal {
        Modal::Help => {
            model.modal = Modal::None;
            return vec![];
        }
        Modal::Pin {
            target,
            input,
            error,
        } => {
            return match key.code {
                KeyCode::Esc => {
                    model.modal = Modal::None;
                    vec![]
                }
                KeyCode::Backspace => {
                    input.pop();
                    vec![]
                }
                KeyCode::Char(c) if c.is_ascii_digit() && input.len() < 8 => {
                    input.push(c);
                    vec![]
                }
                KeyCode::Enter => {
                    let needs_pin = matches!(target, PairTarget::Airplay { .. });
                    if needs_pin && input.is_empty() {
                        *error = Some("Enter the PIN shown by the speaker.".into());
                        return vec![];
                    }
                    *error = None;
                    let cmd = match target {
                        PairTarget::Airplay { device_id, .. } => Cmd::PairAirplay {
                            device_id: device_id.clone(),
                            pin: input.clone(),
                        },
                        PairTarget::Bluetooth { id, .. } => Cmd::PairBluetooth(id.clone()),
                    };
                    model.pending = Some(Pending {
                        label: LABEL_PAIR.into(),
                    });
                    vec![cmd]
                }
                _ => vec![],
            };
        }
        Modal::None => {}
    }
    let Some(action) = action_for(&key) else {
        return vec![];
    };
    match action {
        Action::Quit => {
            model.quit = true;
            return vec![Cmd::Quit];
        }
        Action::Help => {
            model.modal = Modal::Help;
            return vec![];
        }
        Action::Back => {
            model.error = None;
            return vec![];
        }
        Action::Service => {
            let enabled = !model.service_on();
            model.pending = Some(Pending {
                label: LABEL_SERVICE.into(),
            });
            return vec![Cmd::SetService(enabled)];
        }
        Action::Autostart => {
            let Some(current) = model.autostart else {
                return vec![];
            };
            model.pending = Some(Pending {
                label: LABEL_AUTOSTART.into(),
            });
            return vec![Cmd::SetAutostart(!current)];
        }
        _ => {}
    }
    if model.locked() {
        return vec![];
    }
    match action {
        Action::NextPane => {
            model.pane = next_pane(model.pane);
            vec![]
        }
        Action::PrevPane => {
            model.pane = prev_pane(model.pane);
            vec![]
        }
        Action::Down => {
            move_selection(model, 1);
            vec![]
        }
        Action::Up => {
            move_selection(model, -1);
            vec![]
        }
        Action::Select => select(model),
        Action::AdjustUp => adjust(model, 1),
        Action::AdjustDown => adjust(model, -1),
        Action::VolumeUp => set_volume(model, 1),
        Action::VolumeDown => set_volume(model, -1),
        Action::ResetBand => match (model.pane, model.mixer_row) {
            (Pane::Mixer, MixerRow::Eq(band)) => {
                let mut eq = model.snapshot.eq;
                eq[band] = 0.0;
                pending(model, LABEL_MIXER, vec![Cmd::SetEq(eq)])
            }
            _ => vec![],
        },
        Action::Stop => {
            if model.snapshot.active_output.is_some() {
                pending(model, LABEL_STOP, vec![Cmd::DeactivateOutput])
            } else {
                vec![]
            }
        }
        Action::PlayPause => match &model.snapshot.cd {
            Some(cd) if cd.present => {
                let action = if cd.playing {
                    CdAction::Pause
                } else {
                    CdAction::Play
                };
                pending(model, LABEL_CD, vec![Cmd::ControlCd(action)])
            }
            _ => vec![],
        },
        Action::NextTrack => cd_action(model, CdAction::Next),
        Action::PrevTrack => cd_action(model, CdAction::Prev),
        Action::Bluetooth => pending(model, LABEL_SETTINGS, vec![Cmd::OpenBluetoothSettings]),
        Action::Refresh => pending(model, LABEL_REFRESH, vec![Cmd::Refresh]),
        Action::Quit | Action::Help | Action::Back | Action::Service | Action::Autostart => {
            unreachable!("handled above")
        }
    }
}

fn pending(model: &mut Model, label: &str, cmds: Vec<Cmd>) -> Vec<Cmd> {
    model.pending = Some(Pending {
        label: label.into(),
    });
    cmds
}

fn cd_action(model: &mut Model, action: CdAction) -> Vec<Cmd> {
    match &model.snapshot.cd {
        Some(cd) if cd.present => pending(model, LABEL_CD, vec![Cmd::ControlCd(action)]),
        _ => vec![],
    }
}

fn next_pane(pane: Pane) -> Pane {
    match pane {
        Pane::Sources => Pane::Outputs,
        Pane::Outputs => Pane::Mixer,
        Pane::Mixer => Pane::Sources,
    }
}

fn prev_pane(pane: Pane) -> Pane {
    match pane {
        Pane::Sources => Pane::Mixer,
        Pane::Outputs => Pane::Sources,
        Pane::Mixer => Pane::Outputs,
    }
}

fn move_selection(model: &mut Model, delta: i32) {
    match model.pane {
        Pane::Sources => {
            let len = model.snapshot.inputs.len();
            model.source_index = step(model.source_index, delta, len);
        }
        Pane::Outputs => {
            let rows = model.output_rows();
            let len = rows.len();
            if len == 0 {
                model.output_index = 0;
                return;
            }
            let mut index = model.output_index.min(len - 1);
            // Skip headers; a list that is only headers stays put.
            for _ in 0..len {
                let next = step(index, delta, len);
                if next == index {
                    break;
                }
                index = next;
                if matches!(rows[index], OutputRow::Device(_)) {
                    break;
                }
            }
            if matches!(rows[index], OutputRow::Device(_)) {
                model.output_index = index;
            }
        }
        Pane::Mixer => {
            let rows = MixerRow::ALL;
            let current = rows
                .iter()
                .position(|row| *row == model.mixer_row)
                .unwrap_or(0);
            model.mixer_row = rows[step(current, delta, rows.len())];
        }
    }
}

/// Move within `0..len` without wrapping.
fn step(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len as i32 - 1;
    (index as i32 + delta).clamp(0, max) as usize
}

fn clamp_selection(model: &mut Model) {
    let sources = model.snapshot.inputs.len();
    model.source_index = model.source_index.min(sources.saturating_sub(1));
    let rows = model.output_rows();
    if rows.is_empty() {
        model.output_index = 0;
    } else {
        model.output_index = model.output_index.min(rows.len() - 1);
        if matches!(rows[model.output_index], OutputRow::Header(_)) {
            if let Some(first) = rows
                .iter()
                .position(|row| matches!(row, OutputRow::Device(_)))
            {
                model.output_index = first;
            }
        }
    }
}

fn select(model: &mut Model) -> Vec<Cmd> {
    match model.pane {
        Pane::Sources => match model.snapshot.inputs.get(model.source_index) {
            Some(name) => {
                let name = name.clone();
                pending(model, LABEL_SOURCE, vec![Cmd::ActivateInput(name)])
            }
            None => vec![],
        },
        Pane::Outputs => {
            let rows = model.output_rows();
            let Some(OutputRow::Device(index)) = rows.get(model.output_index) else {
                return vec![];
            };
            let output = model.snapshot.outputs[*index].clone();
            if output.needs_pair && !output.paired {
                let target = if output.transport == "airplay" {
                    PairTarget::Airplay {
                        device_id: output.id,
                        name: output.name,
                    }
                } else {
                    PairTarget::Bluetooth {
                        id: output.id,
                        name: output.name,
                    }
                };
                model.modal = Modal::Pin {
                    target,
                    input: String::new(),
                    error: None,
                };
                return vec![];
            }
            pending(
                model,
                LABEL_OUTPUT,
                vec![Cmd::ActivateOutput {
                    transport: output.transport,
                    device_id: output.id,
                }],
            )
        }
        Pane::Mixer => vec![],
    }
}

fn adjust(model: &mut Model, direction: i32) -> Vec<Cmd> {
    if model.pane != Pane::Mixer {
        return vec![];
    }
    match model.mixer_row {
        MixerRow::Volume => set_volume(model, direction),
        MixerRow::Eq(band) => {
            let mut eq = model.snapshot.eq;
            eq[band] =
                (eq[band] + EQ_STEP_DB * direction as f32).clamp(EQ_RANGE_DB.0, EQ_RANGE_DB.1);
            pending(model, LABEL_MIXER, vec![Cmd::SetEq(eq)])
        }
        MixerRow::InputRate | MixerRow::OutputRate => {
            let Some(rates) = &model.snapshot.rates else {
                return vec![];
            };
            let side = if model.mixer_row == MixerRow::InputRate {
                &rates.input
            } else {
                &rates.output
            };
            let Some(next) = next_in_catalog(&side.supported_hz, side.sample_rate_hz, direction)
            else {
                return vec![];
            };
            let cmd = if model.mixer_row == MixerRow::InputRate {
                Cmd::SetSampleRate {
                    input_hz: Some(next),
                    output_hz: None,
                }
            } else {
                Cmd::SetSampleRate {
                    input_hz: None,
                    output_hz: Some(next),
                }
            };
            pending(model, LABEL_MIXER, vec![cmd])
        }
    }
}

/// The catalog entry after or before `current`, wrapping around.
fn next_in_catalog(catalog: &[u32], current: u32, direction: i32) -> Option<u32> {
    if catalog.is_empty() {
        return None;
    }
    let position = catalog.iter().position(|hz| *hz == current).unwrap_or(0);
    let len = catalog.len() as i32;
    let next = (position as i32 + direction).rem_euclid(len) as usize;
    Some(catalog[next])
}

fn set_volume(model: &mut Model, direction: i32) -> Vec<Cmd> {
    let current = model.snapshot.volume as i32;
    let next = (current + VOLUME_STEP as i32 * direction).clamp(0, 100) as u8;
    if next == model.snapshot.volume {
        return vec![];
    }
    pending(model, LABEL_MIXER, vec![Cmd::SetVolume(next)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        ActiveOutput, ActiveOutputView, CdStatus, OutputInfo, OutputPhase, SampleRateResponse,
        SampleRateSide, StatusResponse,
    };

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
        };
        model.autostart = Some(false);
        model
    }

    fn key(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn code(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn live_sonos() -> ActiveOutputView {
        ActiveOutputView {
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
        let rows = m.output_rows();
        assert!(matches!(rows[0], OutputRow::Header("Sonos")));
        assert!(matches!(rows[m.output_index], OutputRow::Device(0)));
        update(&mut m, key('j'));
        assert!(
            matches!(rows[m.output_index], OutputRow::Device(1)),
            "skips the AirPlay header"
        );
        update(&mut m, key('k'));
        assert!(matches!(rows[m.output_index], OutputRow::Device(0)));
        update(&mut m, key('k'));
        assert!(
            matches!(rows[m.output_index], OutputRow::Device(0)),
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

    #[test]
    fn a_successful_pairing_closes_the_modal_and_activates_the_output() {
        let mut m = model_with_mock_snapshot();
        m.modal = Modal::Pin {
            target: PairTarget::Bluetooth {
                id: "bt-unpaired".into(),
                name: "New".into(),
            },
            input: String::new(),
            error: None,
        };
        assert_eq!(
            update(
                &mut m,
                Msg::Done {
                    label: "Pairing".into(),
                    result: Ok(())
                }
            ),
            vec![Cmd::ActivateOutput {
                transport: "bluetooth".into(),
                device_id: "bt-unpaired".into()
            }]
        );
        assert_eq!(m.modal, Modal::None);
    }

    #[test]
    fn h_and_l_adjust_the_focused_mixer_row() {
        let mut m = model_with_mock_snapshot();
        m.pane = Pane::Mixer;
        assert_eq!(update(&mut m, key('l')), vec![Cmd::SetVolume(55)]);
        assert_eq!(update(&mut m, key('h')), vec![Cmd::SetVolume(45)]);
        m.snapshot.volume = 100;
        assert_eq!(update(&mut m, key('l')), vec![], "never above 100");
        m.snapshot.volume = 0;
        assert_eq!(update(&mut m, key('h')), vec![], "never below 0");

        m.mixer_row = MixerRow::Eq(2);
        assert_eq!(
            update(&mut m, key('l')),
            vec![Cmd::SetEq([0.0, 0.0, 1.0, 0.0, 0.0])]
        );
        m.snapshot.eq = [0.0, 0.0, 12.0, 0.0, 0.0];
        assert_eq!(
            update(&mut m, key('l')),
            vec![Cmd::SetEq([0.0, 0.0, 12.0, 0.0, 0.0])],
            "clamped at +12 dB"
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
        m.mixer_row = MixerRow::OutputRate;
        assert_eq!(
            update(&mut m, key('l')),
            vec![Cmd::SetSampleRate {
                input_hz: None,
                output_hz: Some(48000)
            }]
        );
    }

    #[test]
    fn plus_and_minus_change_volume_from_any_pane() {
        let mut m = model_with_mock_snapshot();
        assert_eq!(m.pane, Pane::Sources);
        assert_eq!(update(&mut m, key('+')), vec![Cmd::SetVolume(55)]);
        assert_eq!(update(&mut m, key('-')), vec![Cmd::SetVolume(45)]);
        assert_eq!(update(&mut m, key('=')), vec![Cmd::SetVolume(55)]);
        assert_eq!(update(&mut m, key('_')), vec![Cmd::SetVolume(45)]);
    }

    #[test]
    fn zero_resets_the_focused_band() {
        let mut m = model_with_mock_snapshot();
        m.pane = Pane::Mixer;
        m.mixer_row = MixerRow::Eq(1);
        m.snapshot.eq = [0.0, 3.0, 0.0, 0.0, 0.0];
        assert_eq!(update(&mut m, key('0')), vec![Cmd::SetEq([0.0; 5])]);
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
            vec![Cmd::ControlCd(CdAction::Play)]
        );
        m.snapshot.cd = Some(disc(true));
        assert_eq!(
            update(&mut m, key(' ')),
            vec![Cmd::ControlCd(CdAction::Pause)]
        );
        assert_eq!(
            update(&mut m, key('>')),
            vec![Cmd::ControlCd(CdAction::Next)]
        );
        assert_eq!(
            update(&mut m, key('<')),
            vec![Cmd::ControlCd(CdAction::Prev)]
        );
        assert_eq!(
            update(&mut m, key('.')),
            vec![Cmd::ControlCd(CdAction::Next)]
        );
        assert_eq!(
            update(&mut m, key(',')),
            vec![Cmd::ControlCd(CdAction::Prev)]
        );
    }

    #[test]
    fn s_toggles_the_service_and_locks_the_panes() {
        let mut m = model_with_mock_snapshot();
        assert_eq!(update(&mut m, key('s')), vec![Cmd::SetService(false)]);
        update(
            &mut m,
            Msg::Ws(Incoming::Event(WsEvent::ServiceStateChanged {
                enabled: false,
            })),
        );
        assert!(m.locked());
        assert_eq!(update(&mut m, key('j')), vec![]);
        assert_eq!(update(&mut m, code(KeyCode::Enter)), vec![]);
        assert_eq!(update(&mut m, key('s')), vec![Cmd::SetService(true)]);
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

    #[test]
    fn ws_events_refresh_or_patch_the_snapshot() {
        let mut m = model_with_mock_snapshot();
        let ev = |e| Msg::Ws(Incoming::Event(e));
        assert_eq!(
            update(
                &mut m,
                ev(WsEvent::OutputStateChanged {
                    transport: "sonos".into(),
                    device_name: "Mock Sonos".into(),
                    active: true
                })
            ),
            vec![Cmd::Refresh]
        );
        assert_eq!(
            update(&mut m, ev(WsEvent::ServiceStateChanged { enabled: false })),
            vec![Cmd::Refresh]
        );
        assert!(!m.service_on());
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
        assert_eq!(
            update(
                &mut m,
                ev(WsEvent::DeviceJoined {
                    transport: "sonos".into(),
                    id: "x".into(),
                    name: "X".into()
                })
            ),
            vec![Cmd::Refresh]
        );
        assert_eq!(
            update(
                &mut m,
                ev(WsEvent::DeviceLeft {
                    transport: "sonos".into(),
                    id: "x".into()
                })
            ),
            vec![Cmd::Refresh]
        );
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
            label: "Switching output".into(),
        });
        assert_eq!(
            update(
                &mut m,
                Msg::Done {
                    label: "Switching output".into(),
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
                label: "Switching output".into(),
                result: Err("The speaker did not answer.".into()),
            },
        );
        assert_eq!(m.error.as_deref(), Some("The speaker did not answer."));
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
}
