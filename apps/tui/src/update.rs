//! The pure half of the UI: `update(model, msg) -> commands`. No I/O lives
//! here, so every key, event and reply can be tested without a core.

use crate::api::{CdAction, SampleRateResponse, WsEvent};
use crate::client::{Connection, Incoming};
use crate::keys::{action_for, Action};
use crate::labels::input_labels;
use crate::model::{
    Activation, ActivationStep, DestRow, Meter, MixerControl, MixerRow, Modal, Model, PairTarget,
    Pane, Pending, Snapshot, WriteSlot, METER_FLOOR_DB,
};
use crate::view::{fader_value, CdButton, Target};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Color;

/// The mouse gestures the UI acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down,
    Drag,
    ScrollUp,
    ScrollDown,
}

/// Everything that can happen to the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Key(KeyEvent),
    /// A left click, left drag or wheel turn over `target`, whose screen
    /// region is `rect`.
    Mouse {
        target: Target,
        rect: Rect,
        column: u16,
        row: u16,
        kind: MouseKind,
    },
    Resize(u16, u16),
    Tick,
    /// The 50 ms frame tick that animates the level meter.
    Frame,
    Ws(Incoming),
    Snapshot(Result<Box<Snapshot>, String>),
    Done {
        label: String,
        result: Result<(), String>,
    },
    /// A mixer write finished; see `write`.
    Written {
        control: MixerControl,
        result: Result<(), String>,
    },
    Rates(Result<SampleRateResponse, String>),
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
    /// Re-read both sides' rates after one changed.
    LoadRates,
    SetSampleRate {
        input_hz: Option<u32>,
        output_hz: Option<u32>,
    },
    ControlCd {
        action: CdAction,
        position_ms: Option<u64>,
        track: Option<u8>,
    },
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
pub const VOLUME_FINE_STEP: u8 = 1;
pub const EQ_FINE_STEP_DB: f32 = 0.5;
pub const EQ_RANGE_DB: (f32, f32) = (-12.0, 12.0);

pub const PEAK_HOLD_FRAMES: u8 = 10;
pub const PEAK_FALL_DB: f32 = 1.5;
pub const METER_TIMEOUT_FRAMES: u8 = 20;
/// Core events settle for this many frames before one refresh.
pub const REFRESH_DEBOUNCE_FRAMES: u8 = 6;
/// A refresh while connected, as a safety net for missed events.
pub const BACKGROUND_REFRESH_FRAMES: u16 = 300;

/// A linear level (`0.0..=1.0`) in dBFS, clamped to the meter scale.
pub fn level_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        return METER_FLOOR_DB;
    }
    (20.0 * linear.log10()).clamp(METER_FLOOR_DB, 0.0)
}

pub const LABEL_SOURCE: &str = "Switching source";
pub const LABEL_OUTPUT: &str = "Switching output";
pub const LABEL_STOP: &str = "Stopping";
pub const LABEL_PAIR: &str = "Pairing";
pub const LABEL_CONNECT: &str = "Connecting";
pub const LABEL_MONITOR: &str = "Changing monitor";
pub const LABEL_REFRESH: &str = "Refreshing";
pub const LABEL_AUTOSTART: &str = "Changing autostart";
pub const LABEL_SETTINGS: &str = "Opening Bluetooth settings";
pub const LABEL_CD: &str = "Disc";
/// How far `[` and `]` move the playhead.
pub const CD_SEEK_MS: i64 = 10_000;

/// Where a seek of `delta_ms` from `position_ms` lands, kept inside the track.
pub fn seek_target(position_ms: u64, duration_ms: u64, delta_ms: i64) -> u64 {
    position_ms.saturating_add_signed(delta_ms).min(duration_ms)
}

/// Whether a command talks to `on-air-core`. Quitting and the host-side
/// autostart and settings commands work without it, and a refresh is how
/// the user finds out the core is back.
pub fn needs_core(cmd: &Cmd) -> bool {
    !matches!(
        cmd,
        Cmd::Quit
            | Cmd::Refresh
            | Cmd::QueryAutostart
            | Cmd::SetAutostart(_)
            | Cmd::OpenBluetoothSettings
    )
}

pub const CORE_DOWN_COPY: &str = "on-air-core is not reachable";

/// Runs a user input; while the core is down, an input that would send a core
/// command is undone and says so instead.
fn gated(model: &mut Model, input: impl FnOnce(&mut Model) -> Vec<Cmd>) -> Vec<Cmd> {
    if !model.core_down {
        return input(model);
    }
    let before = model.clone();
    let cmds = input(model);
    if cmds.iter().any(needs_core) {
        *model = before;
        model.notice = Some(CORE_DOWN_COPY.into());
        return vec![];
    }
    cmds
}

pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    match msg {
        Msg::Key(key) => gated(model, |model| on_key(model, key)),
        Msg::Mouse {
            target,
            rect,
            column,
            row,
            kind,
        } => gated(model, |model| {
            on_mouse(model, target, rect, (column, row), kind)
        }),
        Msg::Resize(w, h) => {
            model.size = (w, h);
            vec![]
        }
        Msg::Tick => vec![],
        Msg::Frame => {
            step_meter(&mut model.meter);
            if let Some(left) = model.refresh_in {
                if left <= 1 {
                    model.refresh_in = None;
                    model.since_refresh = 0;
                    return vec![Cmd::Refresh];
                }
                model.refresh_in = Some(left - 1);
            }
            if model.connection == Connection::Open {
                model.since_refresh = model.since_refresh.saturating_add(1);
                if model.since_refresh >= BACKGROUND_REFRESH_FRAMES {
                    model.since_refresh = 0;
                    return vec![Cmd::Refresh];
                }
            }
            vec![]
        }
        Msg::Ws(Incoming::Connection(connection)) => {
            let was_open = model.connection == Connection::Open;
            model.connection = connection;
            // On a drop, one probe tells a restart from a dead core: its
            // failure marks the core down.
            if connection == Connection::Open || was_open {
                vec![Cmd::Refresh]
            } else {
                vec![]
            }
        }
        Msg::Ws(Incoming::Event(event)) => on_event(model, event),
        Msg::Snapshot(Ok(snapshot)) => {
            model.snapshot = *snapshot;
            model.placeholders.clear();
            model.since_refresh = 0;
            model.core_down = false;
            close_cd_modals_without_disc(model);
            model.notice = sleep_notice(&model.snapshot);
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
            model.core_down = true;
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
        Msg::Written { control, result } => on_written(model, control, result),
        Msg::Rates(Ok(rates)) => {
            model.snapshot.rates = Some(rates);
            vec![]
        }
        Msg::Rates(Err(copy)) => {
            model.error = Some(copy);
            vec![]
        }
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

/// A refused sleep lock is worth its own line: the laptop may suspend
/// mid-stream. Nothing to say while the monitor is off or the core is old.
fn sleep_notice(snapshot: &Snapshot) -> Option<String> {
    let status = snapshot.status.as_ref()?;
    (status.service_enabled && status.sleep_inhibited == Some(false))
        .then(|| crate::error_copy::SLEEP_NOT_INHIBITED_COPY.to_string())
}

fn on_done(model: &mut Model, label: &str, result: Result<(), String>) -> Vec<Cmd> {
    let chain_step = match label {
        LABEL_PAIR => Some(ActivationStep::Pairing),
        LABEL_CONNECT => Some(ActivationStep::Connecting),
        LABEL_OUTPUT => Some(ActivationStep::Activating),
        _ => None,
    };
    if let Some(step) = chain_step {
        // A reply for a chain that was cancelled or has moved on: the core
        // may have changed something, so reload, but drive nothing.
        if model.activation.as_ref().map(|a| a.step) != Some(step) {
            if model.pending.as_ref().is_some_and(|p| p.label == label) {
                model.pending = None;
            }
            return vec![Cmd::Refresh];
        }
    }
    model.pending = None;
    match result {
        Ok(()) => {
            model.error = None;
            if chain_step.is_some() {
                let Some(device) = current_device(model) else {
                    return vec![Cmd::Refresh];
                };
                match label {
                    LABEL_PAIR => {
                        // The modal belongs to this chain: only its Enter pairs.
                        if matches!(model.modal, Modal::Pin { .. }) {
                            model.modal = Modal::None;
                        }
                        // Bluetooth must connect before the core can route to it.
                        if device.0 == "bluetooth" {
                            let cmd = Cmd::ConnectBluetooth(device.1.clone());
                            return advance(model, device, ActivationStep::Connecting, cmd);
                        }
                        return activate_step(model, device);
                    }
                    LABEL_CONNECT => return activate_step(model, device),
                    _ => {
                        model.activation = None;
                        model.failed_output = None;
                    }
                }
            }
            if label == LABEL_SETTINGS {
                vec![]
            } else {
                vec![Cmd::Refresh]
            }
        }
        Err(copy) => {
            if label == LABEL_PAIR {
                if let Modal::Pin { error, input, .. } = &mut model.modal {
                    // The modal stays open for another try.
                    *error = Some(copy);
                    input.clear();
                    model.activation = None;
                    return vec![];
                }
            }
            if chain_step.is_some() {
                if let Some(a) = model.activation.take() {
                    model.failed_output = Some((a.transport, a.id));
                }
            }
            model.error = Some(copy);
            vec![]
        }
    }
}

/// A device of the Enter flow: transport, id and name.
type Device = (&'static str, String, String);

/// The device of the activation in progress.
fn current_device(model: &Model) -> Option<Device> {
    let a = model.activation.as_ref()?;
    let transport = match a.transport.as_str() {
        "airplay" => "airplay",
        "bluetooth" => "bluetooth",
        _ => "sonos",
    };
    Some((transport, a.id.clone(), a.name.clone()))
}

/// Start one step of the Enter flow for `device`.
fn advance(model: &mut Model, device: Device, step: ActivationStep, cmd: Cmd) -> Vec<Cmd> {
    let (transport, id, name) = device;
    let label = match step {
        ActivationStep::Pairing => LABEL_PAIR,
        ActivationStep::Connecting => LABEL_CONNECT,
        ActivationStep::Activating => LABEL_OUTPUT,
    };
    model.activation = Some(Activation {
        transport: transport.into(),
        id,
        name,
        step,
    });
    pending(model, label, vec![cmd])
}

fn activate_step(model: &mut Model, device: Device) -> Vec<Cmd> {
    let cmd = Cmd::ActivateOutput {
        transport: device.0.into(),
        device_id: device.1.clone(),
    };
    advance(model, device, ActivationStep::Activating, cmd)
}

/// Send a mixer change, or hold it as the latest value while an earlier
/// write to the same control is still in flight.
fn write(model: &mut Model, control: MixerControl, cmd: Cmd) -> Vec<Cmd> {
    match &mut model.writes[control as usize] {
        slot @ WriteSlot::Idle => {
            *slot = WriteSlot::InFlight { queued: None };
            vec![cmd]
        }
        WriteSlot::InFlight { queued } => {
            *queued = Some(cmd);
            vec![]
        }
    }
}

fn on_written(model: &mut Model, control: MixerControl, result: Result<(), String>) -> Vec<Cmd> {
    let slot = std::mem::replace(&mut model.writes[control as usize], WriteSlot::Idle);
    match result {
        Ok(()) => {
            model.error = None;
            match slot {
                WriteSlot::InFlight { queued: Some(cmd) } => {
                    model.writes[control as usize] = WriteSlot::InFlight { queued: None };
                    vec![cmd]
                }
                _ if matches!(control, MixerControl::InputRate | MixerControl::OutputRate) => {
                    vec![Cmd::LoadRates]
                }
                _ => vec![],
            }
        }
        // The queued value is dropped: reload the core's real values over
        // the optimistic ones instead of sending on top of a failure.
        Err(copy) => {
            model.error = Some(copy);
            vec![Cmd::Refresh]
        }
    }
}

/// Core events arrive in bursts; they settle into one refresh.
fn schedule_refresh(model: &mut Model) -> Vec<Cmd> {
    model.refresh_in.get_or_insert(REFRESH_DEBOUNCE_FRAMES);
    vec![]
}

fn on_event(model: &mut Model, event: WsEvent) -> Vec<Cmd> {
    match event {
        WsEvent::ServiceStateChanged { enabled } => {
            if let Some(status) = &mut model.snapshot.status {
                status.service_enabled = enabled;
            }
            schedule_refresh(model)
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
            close_cd_modals_without_disc(model);
            vec![]
        }
        WsEvent::InputStateChanged {
            active: false,
            error: Some(_),
            ..
        } => {
            model.error = Some(crate::error_copy::INPUT_STOPPED_COPY.to_string());
            schedule_refresh(model)
        }
        WsEvent::OutputStateChanged { .. } | WsEvent::InputStateChanged { .. } => {
            schedule_refresh(model)
        }
        WsEvent::DeviceJoined {
            transport,
            id,
            name,
        } => {
            let listed = model
                .snapshot
                .outputs
                .iter()
                .any(|o| o.transport == transport && o.id == id);
            if !listed
                && !model
                    .placeholders
                    .iter()
                    .any(|p| p.0 == transport && p.1 == id)
            {
                model.placeholders.push((transport, id, name));
            }
            schedule_refresh(model)
        }
        WsEvent::DeviceLeft { transport, id } => {
            model
                .snapshot
                .outputs
                .retain(|o| !(o.transport == transport && o.id == id));
            if transport == "bluetooth" {
                if let Some(devices) = &mut model.snapshot.bluetooth {
                    devices.retain(|d| d.id != id);
                }
            }
            model
                .placeholders
                .retain(|p| !(p.0 == transport && p.1 == id));
            clamp_selection(model);
            schedule_refresh(model)
        }
        WsEvent::LevelMeter { rms, peak } => {
            let live = model
                .snapshot
                .active_output
                .as_ref()
                .is_some_and(|active| active.is_live());
            if model.locked() || !live {
                model.meter = Meter::default();
            } else {
                let peak_db = level_db(peak);
                let meter = &mut model.meter;
                meter.rms_db = level_db(rms);
                if peak_db >= meter.peak_db {
                    meter.peak_db = peak_db;
                    meter.hold = PEAK_HOLD_FRAMES;
                }
                meter.quiet = 0;
            }
            vec![]
        }
        WsEvent::Heartbeat | WsEvent::Unknown => vec![],
    }
}

/// One frame of meter decay: the peak holds, then falls; silence for too
/// long reads zero.
fn step_meter(meter: &mut Meter) {
    meter.quiet = meter.quiet.saturating_add(1);
    if meter.quiet >= METER_TIMEOUT_FRAMES {
        *meter = Meter {
            quiet: METER_TIMEOUT_FRAMES,
            ..Meter::default()
        };
    } else if meter.hold > 0 {
        meter.hold -= 1;
    } else {
        meter.peak_db = (meter.peak_db - PEAK_FALL_DB).max(METER_FLOOR_DB);
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
                    model.activation = None;
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
                // One pairing at a time; the modal says "Pairing…" meanwhile.
                KeyCode::Enter if model.activation.is_some() => vec![],
                KeyCode::Enter => {
                    let needs_pin = matches!(target, PairTarget::Airplay { .. });
                    if needs_pin && input.is_empty() {
                        *error = Some("Enter the PIN shown by the speaker.".into());
                        return vec![];
                    }
                    *error = None;
                    let (transport, id, name, cmd) = match target {
                        PairTarget::Airplay { device_id, name } => (
                            "airplay",
                            device_id.clone(),
                            name.clone(),
                            Cmd::PairAirplay {
                                device_id: device_id.clone(),
                                pin: input.clone(),
                            },
                        ),
                        PairTarget::Bluetooth { id, name } => (
                            "bluetooth",
                            id.clone(),
                            name.clone(),
                            Cmd::PairBluetooth(id.clone()),
                        ),
                    };
                    advance(model, (transport, id, name), ActivationStep::Pairing, cmd)
                }
                _ => vec![],
            };
        }
        Modal::TrackPrompt { input, error } => {
            return match key.code {
                KeyCode::Esc => {
                    model.modal = Modal::None;
                    vec![]
                }
                KeyCode::Backspace => {
                    input.pop();
                    *error = None;
                    vec![]
                }
                KeyCode::Char(c) if c.is_ascii_digit() && input.len() < 3 => {
                    input.push(c);
                    *error = None;
                    vec![]
                }
                KeyCode::Enter => {
                    let Ok(track) = input.parse::<u8>() else {
                        return vec![];
                    };
                    let count = model.snapshot.cd.as_ref().map_or(0, |cd| cd.track_count);
                    if track == 0 || track > count {
                        *error = Some(format!("Track {track} is not on this disc"));
                        return vec![];
                    }
                    model.modal = Modal::None;
                    let cmd = Cmd::ControlCd {
                        action: CdAction::Goto,
                        position_ms: None,
                        track: Some(track),
                    };
                    pending(model, LABEL_CD, vec![cmd])
                }
                _ => vec![],
            };
        }
        Modal::ConfirmEject => {
            return match key.code {
                KeyCode::Char('y') => {
                    model.modal = Modal::None;
                    cd_action(model, CdAction::Eject)
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    model.modal = Modal::None;
                    vec![]
                }
                _ => vec![],
            };
        }
        Modal::Status => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('i')) {
                model.modal = Modal::None;
            }
            return vec![];
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
        Action::Status => {
            model.modal = Modal::Status;
            return vec![];
        }
        Action::Monitor => {
            let enabled = !model.service_on();
            model.pending = Some(Pending {
                label: LABEL_MONITOR.into(),
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
        Action::AdjustUp => adjust(model, 1, false),
        Action::AdjustDown => adjust(model, -1, false),
        Action::FineUp => adjust(model, 1, true),
        Action::FineDown => adjust(model, -1, true),
        Action::VolumeUp => set_volume(model, VOLUME_STEP as i32),
        Action::VolumeDown => set_volume(model, -(VOLUME_STEP as i32)),
        Action::ResetBand => match (model.pane, model.mixer_row) {
            (Pane::Mixer, MixerRow::Eq(band)) => set_eq_band(model, band, 0.0),
            _ => vec![],
        },
        Action::Stop => {
            if model.snapshot.active_output.is_some() {
                pending(model, LABEL_STOP, vec![Cmd::DeactivateOutput])
            } else {
                vec![]
            }
        }
        Action::PlayPause => play_pause(model),
        Action::SeekBack => cd_seek(model, -CD_SEEK_MS),
        Action::SeekForward => cd_seek(model, CD_SEEK_MS),
        Action::GotoTrack => {
            if has_disc(model) {
                model.modal = Modal::TrackPrompt {
                    input: String::new(),
                    error: None,
                };
            }
            vec![]
        }
        Action::Eject => {
            if has_disc(model) {
                model.modal = Modal::ConfirmEject;
            }
            vec![]
        }
        Action::NextTrack => cd_action(model, CdAction::Next),
        Action::PrevTrack => cd_action(model, CdAction::Prev),
        Action::Bluetooth => pending(model, LABEL_SETTINGS, vec![Cmd::OpenBluetoothSettings]),
        Action::Refresh => pending(model, LABEL_REFRESH, vec![Cmd::Refresh]),
        Action::Quit
        | Action::Help
        | Action::Back
        | Action::Status
        | Action::Monitor
        | Action::Autostart => {
            unreachable!("handled above")
        }
    }
}

fn on_mouse(
    model: &mut Model,
    target: Target,
    rect: Rect,
    (column, row): (u16, u16),
    kind: MouseKind,
) -> Vec<Cmd> {
    let click = kind == MouseKind::Down;
    if model.modal != Modal::None {
        if click && target == Target::OverlayOutside {
            close_modal(model);
        }
        return vec![];
    }
    match target {
        Target::MonitorTag if click => {
            let enabled = !model.service_on();
            return pending(model, LABEL_MONITOR, vec![Cmd::SetService(enabled)]);
        }
        Target::StatusTag if click => {
            model.modal = Modal::Status;
            return vec![];
        }
        _ => {}
    }
    if model.locked() {
        return vec![];
    }
    let wheel = match kind {
        MouseKind::ScrollUp => -1,
        MouseKind::ScrollDown => 1,
        MouseKind::Down | MouseKind::Drag => 0,
    };
    match target {
        Target::SourceRow(index) | Target::DestRow(index) => {
            if kind == MouseKind::Drag {
                return vec![];
            }
            model.pane = if matches!(target, Target::SourceRow(_)) {
                Pane::Sources
            } else {
                Pane::Outputs
            };
            if wheel != 0 {
                move_selection(model, wheel);
                return vec![];
            }
            if model.pane == Pane::Sources {
                model.source_index = index;
            } else if matches!(model.dest_rows().get(index), Some(DestRow::Entry(_))) {
                model.output_index = index;
            } else {
                return vec![];
            }
            select(model)
        }
        Target::Fader(mixer_row) => {
            model.pane = Pane::Mixer;
            model.mixer_row = mixer_row;
            if wheel != 0 {
                return adjust(model, -wheel, true);
            }
            // A one-row fader is the compact mixer's line: a click only focuses it.
            if rect.height <= 1 {
                return vec![];
            }
            let offset = row.saturating_sub(rect.y);
            match mixer_row {
                MixerRow::Volume => {
                    let value =
                        fader_value(offset, rect.height, 0.0, 100.0, f32::from(VOLUME_FINE_STEP));
                    let delta = value as i32 - model.snapshot.volume as i32;
                    set_volume(model, delta)
                }
                MixerRow::Eq(band) => {
                    let (min, max) = EQ_RANGE_DB;
                    let value = fader_value(offset, rect.height, min, max, EQ_FINE_STEP_DB);
                    set_eq_band(model, band, value)
                }
                MixerRow::InputRate | MixerRow::OutputRate => vec![],
            }
        }
        Target::Rate(mixer_row) => {
            if kind == MouseKind::Drag {
                return vec![];
            }
            model.pane = Pane::Mixer;
            model.mixer_row = mixer_row;
            adjust(model, if click { 1 } else { -wheel }, false)
        }
        Target::CdButton(button) if click => match button {
            CdButton::Prev => cd_action(model, CdAction::Prev),
            CdButton::Next => cd_action(model, CdAction::Next),
            CdButton::PlayPause => play_pause(model),
            CdButton::Eject => {
                if has_disc(model) {
                    model.modal = Modal::ConfirmEject;
                }
                vec![]
            }
        },
        Target::CdProgress if click => {
            let Some(cd) = model.snapshot.cd.as_ref().filter(|cd| cd.present) else {
                return vec![];
            };
            let width = u64::from(rect.width.max(1));
            let offset = u64::from(column.saturating_sub(rect.x)).min(width);
            let cmd = Cmd::ControlCd {
                action: CdAction::Seek,
                position_ms: Some(cd.duration_ms * offset / width),
                track: None,
            };
            pending(model, LABEL_CD, vec![cmd])
        }
        _ => vec![],
    }
}

/// What Esc does to an open overlay.
fn close_modal(model: &mut Model) {
    if matches!(model.modal, Modal::Pin { .. }) {
        model.activation = None;
    }
    model.modal = Modal::None;
}

fn play_pause(model: &mut Model) -> Vec<Cmd> {
    let playing = model.snapshot.cd.as_ref().is_some_and(|cd| cd.playing);
    cd_action(
        model,
        if playing {
            CdAction::Pause
        } else {
            CdAction::Play
        },
    )
}

fn pending(model: &mut Model, label: &str, cmds: Vec<Cmd>) -> Vec<Cmd> {
    model.pending = Some(Pending {
        label: label.into(),
    });
    cmds
}

fn has_disc(model: &Model) -> bool {
    model.snapshot.cd.as_ref().is_some_and(|cd| cd.present)
}

fn cd_action(model: &mut Model, action: CdAction) -> Vec<Cmd> {
    if !has_disc(model) {
        return vec![];
    }
    let cmd = Cmd::ControlCd {
        action,
        position_ms: None,
        track: None,
    };
    pending(model, LABEL_CD, vec![cmd])
}

fn cd_seek(model: &mut Model, delta_ms: i64) -> Vec<Cmd> {
    let Some(cd) = model.snapshot.cd.as_ref().filter(|cd| cd.present) else {
        return vec![];
    };
    let cmd = Cmd::ControlCd {
        action: CdAction::Seek,
        position_ms: Some(seek_target(cd.position_ms, cd.duration_ms, delta_ms)),
        track: None,
    };
    pending(model, LABEL_CD, vec![cmd])
}

/// A prompt about a disc means nothing once the disc is gone.
fn close_cd_modals_without_disc(model: &mut Model) {
    if !has_disc(model) && matches!(model.modal, Modal::TrackPrompt { .. } | Modal::ConfirmEject) {
        model.modal = Modal::None;
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
            let len = input_labels(&model.snapshot.inputs).len();
            model.source_index = step(model.source_index, delta, len);
        }
        Pane::Outputs => {
            let rows = model.dest_rows();
            let len = rows.len();
            if len == 0 {
                model.output_index = 0;
                return;
            }
            let mut index = model.output_index.min(len - 1);
            // Only entries hold the cursor; a list without any stays put.
            for _ in 0..len {
                let next = step(index, delta, len);
                if next == index {
                    break;
                }
                index = next;
                if matches!(rows[index], DestRow::Entry(_)) {
                    break;
                }
            }
            if matches!(rows[index], DestRow::Entry(_)) {
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
    let sources = input_labels(&model.snapshot.inputs).len();
    model.source_index = model.source_index.min(sources.saturating_sub(1));
    let rows = model.dest_rows();
    if rows.is_empty() {
        model.output_index = 0;
    } else {
        model.output_index = model.output_index.min(rows.len() - 1);
        if !matches!(rows[model.output_index], DestRow::Entry(_)) {
            if let Some(first) = rows.iter().position(|row| matches!(row, DestRow::Entry(_))) {
                model.output_index = first;
            }
        }
    }
}

fn select(model: &mut Model) -> Vec<Cmd> {
    match model.pane {
        Pane::Sources => match input_labels(&model.snapshot.inputs).get(model.source_index) {
            Some(row) => {
                let name = row.name.clone();
                pending(model, LABEL_SOURCE, vec![Cmd::ActivateInput(name)])
            }
            None => vec![],
        },
        Pane::Outputs => {
            let Some(entry) = model.selected_dest() else {
                return vec![];
            };
            if let Some(a) = &model.activation {
                model.notice = Some(format!("Already connecting {}…", a.name));
                return vec![];
            }
            // A retry starts over from the device's real flags.
            model.failed_output = None;
            let output_unpaired = model
                .snapshot
                .outputs
                .iter()
                .find(|o| o.transport == entry.transport && o.id == entry.id)
                .is_some_and(|o| o.needs_pair && !o.paired);
            let listed = if entry.transport == "bluetooth" {
                model.bluetooth_device(&entry.id)
            } else {
                None
            };
            let needs_pair = match (entry.transport, listed) {
                ("bluetooth", Some(d)) => !d.paired,
                _ => output_unpaired,
            };
            let connect = listed.is_some_and(|d| d.paired && !d.connected);
            let device = (entry.transport, entry.id.clone(), entry.name.clone());
            if needs_pair {
                let target = if entry.transport == "airplay" {
                    PairTarget::Airplay {
                        device_id: entry.id,
                        name: entry.name,
                    }
                } else {
                    PairTarget::Bluetooth {
                        id: entry.id,
                        name: entry.name,
                    }
                };
                model.modal = Modal::Pin {
                    target,
                    input: String::new(),
                    error: None,
                };
                return vec![];
            }
            if connect {
                let cmd = Cmd::ConnectBluetooth(entry.id);
                return advance(model, device, ActivationStep::Connecting, cmd);
            }
            activate_step(model, device)
        }
        Pane::Mixer => vec![],
    }
}

fn adjust(model: &mut Model, direction: i32, fine: bool) -> Vec<Cmd> {
    if model.pane != Pane::Mixer {
        return vec![];
    }
    match model.mixer_row {
        MixerRow::Volume => {
            let step = if fine { VOLUME_FINE_STEP } else { VOLUME_STEP };
            set_volume(model, step as i32 * direction)
        }
        MixerRow::Eq(band) => {
            let step = if fine { EQ_FINE_STEP_DB } else { EQ_STEP_DB };
            let gain = model.snapshot.eq[band] + step * direction as f32;
            set_eq_band(model, band, gain)
        }
        MixerRow::InputRate | MixerRow::OutputRate => {
            let input = model.mixer_row == MixerRow::InputRate;
            let Some(rates) = &mut model.snapshot.rates else {
                return vec![];
            };
            let side = if input {
                &mut rates.input
            } else {
                &mut rates.output
            };
            let Some(next) = next_in_catalog(&side.supported_hz, side.sample_rate_hz, direction)
            else {
                return vec![];
            };
            if next == side.sample_rate_hz {
                return vec![];
            }
            side.sample_rate_hz = next;
            if input {
                let cmd = Cmd::SetSampleRate {
                    input_hz: Some(next),
                    output_hz: None,
                };
                write(model, MixerControl::InputRate, cmd)
            } else {
                let cmd = Cmd::SetSampleRate {
                    input_hz: None,
                    output_hz: Some(next),
                };
                write(model, MixerControl::OutputRate, cmd)
            }
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

fn set_volume(model: &mut Model, delta: i32) -> Vec<Cmd> {
    let next = (model.snapshot.volume as i32 + delta).clamp(0, 100) as u8;
    if next == model.snapshot.volume {
        return vec![];
    }
    model.snapshot.volume = next;
    write(model, MixerControl::Volume, Cmd::SetVolume(next))
}

/// Set one band, show it at once, and send the whole curve.
fn set_eq_band(model: &mut Model, band: usize, gain_db: f32) -> Vec<Cmd> {
    let gain_db = gain_db.clamp(EQ_RANGE_DB.0, EQ_RANGE_DB.1);
    if model.snapshot.eq[band] == gain_db {
        return vec![];
    }
    model.snapshot.eq[band] = gain_db;
    let cmd = Cmd::SetEq(model.snapshot.eq);
    write(model, MixerControl::Eq, cmd)
}

#[cfg(test)]
mod tests {
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
        update(&mut m, code(KeyCode::Down));
        update(&mut m, code(KeyCode::Down));
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
            vec![]
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
    fn drag_on_eq_fader_sets_value_through_write() {
        let mut m = model_with_mock_snapshot();
        let rect = Rect::new(30, 8, 3, 5);
        let fader = Target::Fader(MixerRow::Eq(2));
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
}
