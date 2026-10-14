//! The whole UI state in one value. Everything shown comes from the core's
//! snapshot; the rest is focus, modals, one in-flight action and the last
//! error.

use crate::api::{
    ActiveOutputView, BluetoothDeviceInfo, CdStatus, OutputInfo, OutputPhase, SampleRateResponse,
    StatusResponse,
};
use crate::client::Connection;
use crate::update::Cmd;
use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Sources,
    Outputs,
    Mixer,
}

/// Rows of the Mixer pane, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerRow {
    Volume,
    /// EQ band index, `0..5`.
    Eq(usize),
    InputRate,
    OutputRate,
}

impl MixerRow {
    pub const ALL: [MixerRow; 8] = [
        MixerRow::Volume,
        MixerRow::Eq(0),
        MixerRow::Eq(1),
        MixerRow::Eq(2),
        MixerRow::Eq(3),
        MixerRow::Eq(4),
        MixerRow::InputRate,
        MixerRow::OutputRate,
    ];
}

/// A mixer control with its own write slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerControl {
    Volume,
    Eq,
    InputRate,
    OutputRate,
}

/// Where a control's write stands. While one is in flight, a change only
/// replaces `queued`, so the core sees the latest value, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteSlot {
    Idle,
    InFlight { queued: Option<Cmd> },
}

/// What the pairing modal is pairing.
#[derive(Debug, Clone, PartialEq)]
pub enum PairTarget {
    Airplay { device_id: String, name: String },
    Bluetooth { id: String, name: String },
}

impl PairTarget {
    pub fn name(&self) -> &str {
        match self {
            PairTarget::Airplay { name, .. } | PairTarget::Bluetooth { name, .. } => name,
        }
    }

    /// AirPlay receivers show a PIN; Bluetooth pairing is a confirmation.
    pub fn needs_pin(&self) -> bool {
        matches!(self, PairTarget::Airplay { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    None,
    Help,
    Pin {
        target: PairTarget,
        input: String,
        error: Option<String>,
    },
    /// Digits of the CD track to go to.
    TrackPrompt {
        input: String,
        error: Option<String>,
    },
    ConfirmEject,
    /// The `i` info overlay.
    Status,
}

/// One action in flight, shown in the header until its `Done` arrives.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub label: String,
}

/// Everything the core reports, as last loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub status: Option<StatusResponse>,
    pub pin: Option<String>,
    pub inputs: Vec<String>,
    pub active_input: Option<String>,
    pub outputs: Vec<OutputInfo>,
    pub active_output: Option<ActiveOutputView>,
    pub volume: u8,
    pub eq: [f32; 5],
    pub rates: Option<SampleRateResponse>,
    pub cd: Option<CdStatus>,
    pub airplay_mode: Option<String>,
    /// Known Bluetooth devices; `None` when the list failed to load.
    pub bluetooth: Option<Vec<BluetoothDeviceInfo>>,
}

/// What a Destination row is doing, in display precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestState {
    Live,
    Connecting,
    Pairing,
    Failed,
    Pin,
    Connected,
    Paired,
    NotPaired,
    Idle,
}

/// A selectable device in the Destination pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestEntry {
    pub transport: &'static str,
    pub id: String,
    pub name: String,
    /// A stereo pair or a group of two or more speakers.
    pub pair_badge: bool,
    pub state: DestState,
    /// A device shown while discovery has not listed it yet.
    pub placeholder: bool,
}

/// A row of the Destination pane: a transport header, a device or the
/// help line of an empty group. Only `Entry` rows hold the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestRow {
    Header(&'static str),
    Entry(DestEntry),
    Empty(&'static str),
}

/// Which step of an Enter on a Destination row is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationStep {
    Pairing,
    Connecting,
    Activating,
}

/// The Enter flow in progress for one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub transport: String,
    pub id: String,
    pub name: String,
    pub step: ActivationStep,
}

/// Transports in display order with their header labels.
pub const TRANSPORTS: [(&str, &str); 3] = [
    ("sonos", "Sonos"),
    ("airplay", "AirPlay"),
    ("bluetooth", "Bluetooth"),
];

/// The bottom of the meter scale, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;

/// The level meter: RMS and held peak in dBFS, and the frame counters that
/// drive the peak fall and the timeout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub rms_db: f32,
    pub peak_db: f32,
    /// Frames left before the peak starts to fall.
    pub hold: u8,
    /// Frames since the last level event.
    pub quiet: u8,
}

impl Default for Meter {
    fn default() -> Self {
        Meter {
            rms_db: METER_FLOOR_DB,
            peak_db: METER_FLOOR_DB,
            hold: 0,
            quiet: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub connection: Connection,
    pub snapshot: Snapshot,
    pub pane: Pane,
    pub source_index: usize,
    /// Index into `dest_rows()`; never a header once there is a device.
    pub output_index: usize,
    pub mixer_row: MixerRow,
    /// Indexed by `MixerControl as usize`.
    pub writes: [WriteSlot; 4],
    pub meter: Meter,
    pub activation: Option<Activation>,
    /// The (transport, id) whose last activation failed.
    pub failed_output: Option<(String, String)>,
    /// (transport, id, name) of devices shown before discovery lists them.
    pub placeholders: Vec<(String, String, String)>,
    /// Frames until a debounced refresh; set by core events.
    pub refresh_in: Option<u8>,
    /// Frames since the last snapshot.
    pub since_refresh: u16,
    pub modal: Modal,
    pub pending: Option<Pending>,
    pub error: Option<String>,
    pub notice: Option<String>,
    /// `None` until the host answers, or where autostart is not ours.
    pub autostart: Option<bool>,
    /// The last snapshot failed: the panes keep their rows but are dimmed.
    pub core_down: bool,
    /// The core's base URL, shown in the status overlay.
    pub base: String,
    pub accent: Color,
    pub size: (u16, u16),
    pub quit: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    pub fn new() -> Self {
        Model {
            connection: Connection::Connecting,
            snapshot: Snapshot::default(),
            pane: Pane::Sources,
            source_index: 0,
            output_index: 1,
            mixer_row: MixerRow::Volume,
            writes: [const { WriteSlot::Idle }; 4],
            meter: Meter::default(),
            activation: None,
            failed_output: None,
            placeholders: Vec::new(),
            refresh_in: None,
            since_refresh: 0,
            modal: Modal::None,
            pending: None,
            error: None,
            notice: None,
            autostart: None,
            core_down: false,
            base: String::new(),
            accent: Color::Cyan,
            size: (0, 0),
            quit: false,
        }
    }

    /// Unknown status counts as on, so a core that has not answered yet does
    /// not lock the screen.
    pub fn service_on(&self) -> bool {
        self.snapshot
            .status
            .as_ref()
            .is_none_or(|status| status.service_enabled)
    }

    pub fn locked(&self) -> bool {
        !self.service_on()
    }

    /// Destinations grouped under transport headers in `TRANSPORTS` order.
    /// An empty group gets a help line so the pane reads the same every
    /// time; Bluetooth lists its outputs first, then list-only devices.
    pub fn dest_rows(&self) -> Vec<DestRow> {
        let mut rows = Vec::new();
        for (transport, label) in TRANSPORTS {
            rows.push(DestRow::Header(label));
            let start = rows.len();
            let outputs = || {
                self.snapshot
                    .outputs
                    .iter()
                    .filter(move |o| o.transport == transport)
            };
            for output in outputs() {
                let device = if transport == "bluetooth" {
                    self.bluetooth_device(&output.id)
                } else {
                    None
                };
                rows.push(DestRow::Entry(DestEntry {
                    transport,
                    id: output.id.clone(),
                    name: output.name.clone(),
                    pair_badge: output.kind == "pair" || output.member_count >= 2,
                    state: self.dest_state(transport, &output.id, Some(output), device),
                    placeholder: false,
                }));
            }
            if transport == "bluetooth" {
                for device in self.snapshot.bluetooth.iter().flatten() {
                    if outputs().any(|o| o.id == device.id) {
                        continue;
                    }
                    rows.push(DestRow::Entry(DestEntry {
                        transport,
                        id: device.id.clone(),
                        name: device.name.clone(),
                        pair_badge: false,
                        state: self.dest_state(transport, &device.id, None, Some(device)),
                        placeholder: false,
                    }));
                }
                self.push_placeholders(transport, &mut rows);
                if self.snapshot.bluetooth.is_none() {
                    rows.push(DestRow::Empty("Bluetooth list unavailable"));
                    continue;
                }
            }
            if transport != "bluetooth" {
                self.push_placeholders(transport, &mut rows);
            }
            if rows.len() == start {
                rows.push(DestRow::Empty(match transport {
                    "sonos" => "None found on this network — press r to rescan",
                    "airplay" => "None found — AirPlay needs OwnTone or pyatv",
                    _ => "None paired — press b for Bluetooth settings",
                }));
            }
        }
        rows
    }

    /// The device under the Destination cursor, if the cursor is on one.
    pub fn selected_dest(&self) -> Option<DestEntry> {
        match self.dest_rows().into_iter().nth(self.output_index) {
            Some(DestRow::Entry(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn bluetooth_device(&self, id: &str) -> Option<&BluetoothDeviceInfo> {
        self.snapshot
            .bluetooth
            .as_ref()?
            .iter()
            .find(|d| d.id == id)
    }

    /// The state of one device; the first rule that matches wins.
    /// Devices announced by the core before the next snapshot lists them.
    fn push_placeholders(&self, transport: &'static str, rows: &mut Vec<DestRow>) {
        let listed = |id: &str| {
            self.snapshot
                .outputs
                .iter()
                .any(|o| o.transport == transport && o.id == id)
                || (transport == "bluetooth"
                    && self.snapshot.bluetooth.iter().flatten().any(|d| d.id == id))
        };
        for (_, id, name) in self.placeholders.iter().filter(|p| p.0 == transport) {
            if listed(id) {
                continue;
            }
            rows.push(DestRow::Entry(DestEntry {
                transport,
                id: id.clone(),
                name: name.clone(),
                pair_badge: false,
                state: DestState::Idle,
                placeholder: true,
            }));
        }
    }

    fn dest_state(
        &self,
        transport: &str,
        id: &str,
        output: Option<&OutputInfo>,
        device: Option<&BluetoothDeviceInfo>,
    ) -> DestState {
        let is_this = |t: &str, i: &str| t == transport && i == id;
        if let Some(active) = &self.snapshot.active_output {
            if is_this(&active.output.transport, &active.output.device_id) {
                return match active.state {
                    Some(OutputPhase::Failed) => DestState::Failed,
                    Some(OutputPhase::Starting) => DestState::Connecting,
                    Some(OutputPhase::Live) | None => DestState::Live,
                };
            }
        }
        if let Some(activation) = &self.activation {
            if is_this(&activation.transport, &activation.id) {
                return match activation.step {
                    ActivationStep::Pairing => DestState::Pairing,
                    ActivationStep::Connecting | ActivationStep::Activating => {
                        DestState::Connecting
                    }
                };
            }
        }
        if let Some((t, i)) = &self.failed_output {
            if is_this(t, i) {
                return DestState::Failed;
            }
        }
        if transport == "airplay" && output.is_some_and(|o| o.needs_pair && !o.paired) {
            return DestState::Pin;
        }
        if transport == "bluetooth" {
            // Without a list entry, the output's own flag is all there is.
            let (paired, connected) = match (device, output) {
                (Some(d), _) => (d.paired, d.connected),
                (None, Some(o)) => (o.paired, false),
                (None, None) => (false, false),
            };
            return match (paired, connected) {
                (_, true) => DestState::Connected,
                (true, false) => DestState::Paired,
                (false, false) => DestState::NotPaired,
            };
        }
        DestState::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ActiveOutput;

    fn output(id: &str, transport: &str) -> OutputInfo {
        OutputInfo {
            id: id.into(),
            name: id.into(),
            transport: transport.into(),
            kind: "solo".into(),
            member_count: 1,
            needs_pair: false,
            paired: true,
        }
    }

    fn device(id: &str, paired: bool, connected: bool) -> BluetoothDeviceInfo {
        BluetoothDeviceInfo {
            id: id.into(),
            name: id.into(),
            paired,
            connected,
        }
    }

    fn active(transport: &str, id: &str, state: Option<OutputPhase>) -> ActiveOutputView {
        ActiveOutputView {
            output: ActiveOutput {
                transport: transport.into(),
                device_id: id.into(),
                device_name: id.into(),
            },
            state,
        }
    }

    fn entries(model: &Model, transport: &str) -> Vec<DestEntry> {
        model
            .dest_rows()
            .into_iter()
            .filter_map(|row| match row {
                DestRow::Entry(e) if e.transport == transport => Some(e),
                _ => None,
            })
            .collect()
    }

    fn state_of(model: &Model, transport: &str, id: &str) -> DestState {
        entries(model, transport)
            .into_iter()
            .find(|e| e.id == id)
            .unwrap()
            .state
    }

    fn activation(step: ActivationStep) -> Option<Activation> {
        Some(Activation {
            transport: "sonos".into(),
            id: "s1".into(),
            name: "s1".into(),
            step,
        })
    }

    #[test]
    fn dest_state_precedence() {
        let mut m = Model::new();
        m.snapshot.outputs = vec![output("s1", "sonos")];
        for (phase, want) in [
            (Some(OutputPhase::Failed), DestState::Failed),
            (Some(OutputPhase::Starting), DestState::Connecting),
            (Some(OutputPhase::Live), DestState::Live),
        ] {
            m.snapshot.active_output = Some(active("sonos", "s1", phase));
            // The active output beats an activation and a failure.
            m.activation = activation(ActivationStep::Pairing);
            m.failed_output = Some(("sonos".into(), "s1".into()));
            assert_eq!(state_of(&m, "sonos", "s1"), want);
        }
        m.snapshot.active_output = None;
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Pairing);
        m.activation = activation(ActivationStep::Connecting);
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Connecting);
        m.activation = activation(ActivationStep::Activating);
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Connecting);
        m.activation = None;
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Failed);
        m.failed_output = None;
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Idle);

        let mut airplay = output("ap", "airplay");
        airplay.needs_pair = true;
        airplay.paired = false;
        m.snapshot.outputs.push(airplay);
        assert_eq!(state_of(&m, "airplay", "ap"), DestState::Pin);
        m.failed_output = Some(("airplay".into(), "ap".into()));
        assert_eq!(state_of(&m, "airplay", "ap"), DestState::Failed);
        m.failed_output = None;

        m.snapshot.outputs.push(output("bt-c", "bluetooth"));
        m.snapshot.outputs.push(output("bt-p", "bluetooth"));
        m.snapshot.outputs.push(output("bt-n", "bluetooth"));
        m.snapshot.bluetooth = Some(vec![
            device("bt-c", true, true),
            device("bt-p", true, false),
            device("bt-n", false, false),
        ]);
        assert_eq!(state_of(&m, "bluetooth", "bt-c"), DestState::Connected);
        assert_eq!(state_of(&m, "bluetooth", "bt-p"), DestState::Paired);
        assert_eq!(state_of(&m, "bluetooth", "bt-n"), DestState::NotPaired);
    }

    #[test]
    fn bluetooth_list_merges_by_id() {
        let mut m = Model::new();
        m.snapshot.outputs = vec![output("bt-speaker", "bluetooth")];
        m.snapshot.bluetooth = Some(vec![
            device("bt-speaker", true, true),
            device("bt-new", false, false),
        ]);
        let found = entries(&m, "bluetooth");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "bt-speaker");
        assert_eq!(found[0].state, DestState::Connected);
        assert_eq!(found[1].id, "bt-new");
        assert_eq!(found[1].state, DestState::NotPaired);
    }

    #[test]
    fn bluetooth_list_failure_adds_unavailable_line() {
        let mut m = Model::new();
        m.snapshot.outputs = vec![output("bt-speaker", "bluetooth")];
        m.snapshot.bluetooth = None;
        assert_eq!(
            m.dest_rows().last(),
            Some(&DestRow::Empty("Bluetooth list unavailable"))
        );
    }

    #[test]
    fn empty_groups_get_help_lines() {
        let mut m = Model::new();
        m.snapshot.bluetooth = Some(vec![]);
        assert_eq!(
            m.dest_rows(),
            vec![
                DestRow::Header("Sonos"),
                DestRow::Empty("None found on this network — press r to rescan"),
                DestRow::Header("AirPlay"),
                DestRow::Empty("None found — AirPlay needs OwnTone or pyatv"),
                DestRow::Header("Bluetooth"),
                DestRow::Empty("None paired — press b for Bluetooth settings"),
            ]
        );
        m.snapshot.bluetooth = None;
        assert_eq!(
            m.dest_rows().last(),
            Some(&DestRow::Empty("Bluetooth list unavailable"))
        );
    }

    #[test]
    fn pair_badge_for_pairs() {
        let mut m = Model::new();
        let mut pair = output("a", "sonos");
        pair.kind = "pair".into();
        let mut group = output("b", "sonos");
        group.member_count = 2;
        m.snapshot.outputs = vec![pair, group, output("c", "sonos")];
        let badges: Vec<bool> = entries(&m, "sonos").iter().map(|e| e.pair_badge).collect();
        assert_eq!(badges, vec![true, true, false]);
    }
}
