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
    Benchmark(Result<crate::api::AudioBenchmark, String>),
    /// The first-run tuning status behind the audio-check surface.
    Tuning(Result<crate::api::TuningStatus, String>),
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
    RepairFirewall,
    RunAudioCheck,
    SetCaptureBuffer(u32),
    /// Read the first-run tuning status for the audio-check surface.
    FetchTuning,
    SetDspQuality(crate::api::DspQuality),
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
pub const LABEL_FIREWALL: &str = "Authorizing firewall rule";
pub const LABEL_BENCHMARK: &str = "Checking audio performance";
pub const LABEL_BUFFER: &str = "Changing capture buffer";
pub const LABEL_QUALITY: &str = "Changing DSP quality";
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
        Msg::Benchmark(result) => {
            model.pending = None;
            match result {
                Ok(report) => {
                    model.buffer_choice_ms = report.recommended_buffer_ms;
                    model.benchmark = Some(report);
                    model.error = None;
                    // A completed check marks the machine as tuned: the
                    // audio-check surface's tuning line must catch up.
                    vec![Cmd::FetchTuning]
                }
                Err(error) => {
                    model.error = Some(error);
                    vec![]
                }
            }
        }
        Msg::Tuning(result) => {
            match result {
                Ok(tuning) => {
                    model.quality_choice = tuning.quality;
                    model.tuning = Some(tuning);
                }
                Err(error) => model.error = Some(error),
            }
            vec![]
        }
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
            let previous = std::mem::replace(&mut model.snapshot, *snapshot);
            keep_in_flight(model, previous);
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

/// A snapshot read while a mixer write is in flight holds the old value:
/// keep the optimistic one until the write lands.
fn keep_in_flight(model: &mut Model, previous: Snapshot) {
    let in_flight = |control: MixerControl| {
        matches!(model.writes[control as usize], WriteSlot::InFlight { .. })
    };
    let (volume, eq, input, output) = (
        in_flight(MixerControl::Volume),
        in_flight(MixerControl::Eq),
        in_flight(MixerControl::InputRate),
        in_flight(MixerControl::OutputRate),
    );
    let snapshot = &mut model.snapshot;
    if volume {
        snapshot.volume = previous.volume;
    }
    if eq {
        snapshot.eq = previous.eq;
    }
    if let (Some(rates), Some(old)) = (&mut snapshot.rates, previous.rates) {
        if input {
            rates.input.sample_rate_hz = old.input.sample_rate_hz;
        }
        if output {
            rates.output.sample_rate_hz = old.output.sample_rate_hz;
        }
    }
}

/// A refused sleep lock is worth its own line: the laptop may suspend
/// mid-stream. Nothing to say while the monitor is off or the core is old.
fn sleep_notice(snapshot: &Snapshot) -> Option<String> {
    let status = snapshot.status.as_ref()?;
    if let Some(warning) = &status.firewall_warning {
        return Some(format!("{} Press f for help.", warning.message));
    }
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
            if label == LABEL_FIREWALL {
                model.modal = Modal::None;
            }
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
                let mut cmds = vec![Cmd::Refresh];
                if matches!(label, LABEL_BUFFER | LABEL_QUALITY) {
                    cmds.push(Cmd::FetchTuning);
                }
                cmds
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
        Ok(()) => match slot {
            WriteSlot::InFlight { queued: Some(cmd) } => {
                model.writes[control as usize] = WriteSlot::InFlight { queued: None };
                vec![cmd]
            }
            _ if matches!(control, MixerControl::InputRate | MixerControl::OutputRate) => {
                vec![Cmd::LoadRates]
            }
            _ => vec![],
        },
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
                clamp_selection(model);
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
        Modal::AudioCheck => {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('t') => {
                    model.modal = Modal::None;
                    vec![]
                }
                KeyCode::Enter if model.pending.is_none() => {
                    pending(model, LABEL_BENCHMARK, vec![Cmd::RunAudioCheck])
                }
                KeyCode::Char('a') if model.pending.is_none() => {
                    let mut cmds = vec![Cmd::SetCaptureBuffer(model.buffer_choice_ms)];
                    // The quality choice applies only when the current one
                    // is known and different: never clobber a setting the
                    // surface has not loaded yet.
                    if model
                        .tuning
                        .as_ref()
                        .is_some_and(|t| t.quality != model.quality_choice)
                    {
                        cmds.push(Cmd::SetDspQuality(model.quality_choice));
                    }
                    pending(model, LABEL_BUFFER, cmds)
                }
                KeyCode::Char('q') if model.pending.is_none() => {
                    model.quality_choice = model.quality_choice.next();
                    vec![]
                }
                KeyCode::Left | KeyCode::Right if model.pending.is_none() => {
                    model.buffer_choice_ms = next_in_catalog(
                        &[0, 50, 100, 200, 400],
                        model.buffer_choice_ms,
                        if key.code == KeyCode::Right { 1 } else { -1 },
                    )
                    .unwrap_or(0);
                    vec![]
                }
                _ => vec![],
            };
        }
        Modal::Firewall => {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('f') => {
                    model.modal = Modal::None;
                    vec![]
                }
                KeyCode::Enter
                    if model.pending.is_none() && crate::handoff::is_loopback_base(&model.base) =>
                {
                    pending(model, LABEL_FIREWALL, vec![Cmd::RepairFirewall])
                }
                _ => vec![],
            };
        }
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
                    let Ok(number) = input.parse::<u16>() else {
                        return vec![];
                    };
                    let count = model.snapshot.cd.as_ref().map_or(0, |cd| cd.track_count);
                    let track = match u8::try_from(number) {
                        Ok(track) if track != 0 && track <= count => track,
                        _ => {
                            *error = Some(format!("Track {number} is not on this disc"));
                            return vec![];
                        }
                    };
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
            match key.code {
                KeyCode::Esc | KeyCode::Char('i') => model.modal = Modal::None,
                KeyCode::Char('q') => {
                    model.quit = true;
                    return vec![Cmd::Quit];
                }
                _ => {}
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
            model.notice = None;
            return vec![];
        }
        Action::Status => {
            model.modal = Modal::Status;
            return vec![];
        }
        Action::Firewall => {
            if model
                .snapshot
                .status
                .as_ref()
                .is_some_and(|s| s.firewall_warning.is_some())
            {
                model.modal = Modal::Firewall;
            }
            return vec![];
        }
        Action::AudioCheck => return open_audio_check(model),
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
        | Action::Firewall
        | Action::AudioCheck
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
        Target::AudioCheck if click => return open_audio_check(model),
        Target::Firewall if click => {
            model.modal = Modal::Firewall;
            return vec![];
        }
        Target::Refresh if click => {
            return pending(model, LABEL_REFRESH, vec![Cmd::Refresh]);
        }
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
                remember_dest(model);
            } else {
                return vec![];
            }
            select(model)
        }
        Target::Fader(mixer_row) => {
            let focused = model.pane == Pane::Mixer && model.mixer_row == mixer_row;
            model.pane = Pane::Mixer;
            model.mixer_row = mixer_row;
            if wheel != 0 {
                return adjust(model, -wheel, true);
            }
            // The first click on a fader focuses it; the next one, or a drag
            // from it, sets it. A one-row fader is the compact mixer's line:
            // a click only focuses it.
            if (click && !focused) || rect.height <= 1 {
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

fn open_audio_check(model: &mut Model) -> Vec<Cmd> {
    model.buffer_choice_ms = model
        .snapshot
        .status
        .as_ref()
        .and_then(|s| s.capture_buffer_ms)
        .unwrap_or(0);
    if let Some(tuning) = model.tuning.as_ref() {
        model.quality_choice = tuning.quality;
    }
    model.modal = Modal::AudioCheck;
    vec![Cmd::FetchTuning]
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
                remember_dest(model);
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

/// Remember the device under the Destination cursor, so the cursor can
/// follow it when rows above it come and go.
fn remember_dest(model: &mut Model) {
    model.selected_dest_key = model
        .selected_dest()
        .map(|entry| (entry.transport.to_string(), entry.id));
}

/// Keep the cursors on real rows after the lists change; the Destination
/// cursor stays on its device while that device is listed.
fn clamp_selection(model: &mut Model) {
    let sources = input_labels(&model.snapshot.inputs).len();
    model.source_index = model.source_index.min(sources.saturating_sub(1));
    let rows = model.dest_rows();
    let kept = model
        .selected_dest_key
        .as_ref()
        .and_then(|(transport, id)| {
            rows.iter().position(
                |row| matches!(row, DestRow::Entry(e) if e.transport == transport && e.id == *id),
            )
        });
    if let Some(index) = kept {
        model.output_index = index;
    } else if rows.is_empty() {
        model.output_index = 0;
    } else {
        model.output_index = model.output_index.min(rows.len() - 1);
        if !matches!(rows[model.output_index], DestRow::Entry(_)) {
            if let Some(first) = rows.iter().position(|row| matches!(row, DestRow::Entry(_))) {
                model.output_index = first;
            }
        }
    }
    if kept.is_none() {
        remember_dest(model);
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
            // A placeholder waits for the next snapshot to list it.
            let Some(entry) = model.selected_dest().filter(|e| !e.placeholder) else {
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
mod tests;
