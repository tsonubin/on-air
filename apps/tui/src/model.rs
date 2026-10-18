//! The whole UI state in one value. Everything shown comes from the core's
//! snapshot; the rest is focus, modals, one in-flight action and the last
//! error.

use crate::api::{
    ActiveOutputView, BluetoothDeviceInfo, CdStatus, DspQuality, OutputInfo, OutputPhase,
    OutputSyncStatus, SampleRateResponse, StatusResponse, SyncGroupView, SyncState,
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
    /// The manual latency trim of one output, typed in milliseconds.
    TrimPrompt {
        transport: String,
        device_id: String,
        name: String,
        input: String,
        error: Option<String>,
    },
    /// Sync-group formation for one output: the highlighted group in
    /// `Snapshot.sync_groups`, or a new group's name being typed.
    GroupPrompt {
        transport: String,
        device_id: String,
        name: String,
        /// Highlighted row in the group list.
        index: usize,
        input: String,
        /// Typing a new group's name (`n`) instead of picking one.
        naming: bool,
        error: Option<String>,
    },
    ConfirmEject,
    /// The `i` info overlay.
    Status,
    Firewall,
    AudioCheck,
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
    /// The DSP quality profile: current, recommended and selectable.
    pub audio_quality: Option<crate::api::AudioQualityResponse>,
    /// Per-output latency trim and sync state; `None` on an older core.
    pub sync: Option<Vec<OutputSyncStatus>>,
    /// The sync groups with their members; `None` on an older core whose
    /// sync response carried no groups.
    pub sync_groups: Option<Vec<SyncGroupView>>,
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
    /// Its latency trim and sync state, when the core reports them.
    pub sync: Option<SyncBadge>,
    /// The name of the sync group it plays with, when it is in one.
    pub group: Option<String>,
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
pub const TRANSPORTS: [(&str, &str); 4] = [
    ("local", "This computer"),
    ("sonos", "Sonos"),
    ("airplay", "AirPlay"),
    ("bluetooth", "Bluetooth"),
];

/// The bottom of the meter scale, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;

/// One channel's level meter: RMS and held peak in dBFS, and the frame
/// counters that drive the peak fall and the timeout.
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

/// The stereo level meter: one [`Meter`] per channel. A mono source
/// duplicates its one channel into both.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StereoMeter {
    pub left: Meter,
    pub right: Meter,
}

/// How one output's sync stands, as the Destination row shows it. Drift is
/// rounded to whole ppm; the trim is a manual correction, never a
/// measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncBadge {
    pub state: SyncState,
    pub drift_ppm: Option<i64>,
    pub trim_ms: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub benchmark: Option<crate::api::AudioBenchmark>,
    /// The first-run tuning status behind the audio-check surface.
    pub tuning: Option<crate::api::TuningStatus>,
    /// The DSP quality profile the Audio performance overlay has selected.
    pub quality_choice: DspQuality,
    pub buffer_choice_ms: u32,
    pub connection: Connection,
    pub snapshot: Snapshot,
    pub pane: Pane,
    pub source_index: usize,
    /// Index into `dest_rows()`; never a header once there is a device.
    pub output_index: usize,
    /// The (transport, id) under the Destination cursor; `output_index`
    /// follows it when rows come and go.
    pub selected_dest_key: Option<(String, String)>,
    pub mixer_row: MixerRow,
    /// Indexed by `MixerControl as usize`.
    pub writes: [WriteSlot; 4],
    pub meter: StereoMeter,
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
    /// Whether the terminal granted mouse capture.
    pub mouse: bool,
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
            benchmark: None,
            tuning: None,
            quality_choice: DspQuality::Balanced,
            buffer_choice_ms: 0,
            connection: Connection::Connecting,
            snapshot: Snapshot::default(),
            pane: Pane::Sources,
            source_index: 0,
            output_index: 1,
            selected_dest_key: None,
            mixer_row: MixerRow::Volume,
            writes: [const { WriteSlot::Idle }; 4],
            meter: StereoMeter::default(),
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
            mouse: true,
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
            // The computer's own outputs exist only on Linux with PipeWire;
            // elsewhere the group would only ever be empty.
            if transport == "local" && !self.snapshot.outputs.iter().any(|o| o.transport == "local")
            {
                continue;
            }
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
                    sync: self.sync_badge(transport, &output.id),
                    group: self.sync_group_name(transport, &output.id),
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
                        sync: self.sync_badge(transport, &device.id),
                        group: self.sync_group_name(transport, &device.id),
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

    /// The sync status row of one output, when the core reports it.
    pub fn sync_row(&self, transport: &str, id: &str) -> Option<&OutputSyncStatus> {
        self.snapshot
            .sync
            .as_ref()?
            .iter()
            .find(|row| row.transport == transport && row.device_id == id)
    }

    /// One output's sync badge for the Destination row.
    fn sync_badge(&self, transport: &str, id: &str) -> Option<SyncBadge> {
        let row = self.sync_row(transport, id)?;
        Some(SyncBadge {
            state: row.state,
            drift_ppm: row.drift_ppm.map(|ppm| ppm.round() as i64),
            trim_ms: row.trim_ms,
        })
    }

    /// The name of the sync group this output plays with, when its group
    /// is one the core has listed.
    fn sync_group_name(&self, transport: &str, id: &str) -> Option<String> {
        let group_id = self.sync_row(transport, id)?.group_id?;
        Some(
            self.snapshot
                .sync_groups
                .as_ref()?
                .iter()
                .find(|group| group.id == group_id)?
                .name
                .clone(),
        )
    }

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
                sync: None,
                group: None,
            }));
        }
    }

    /// The state of one device; the first rule that matches wins.
    fn dest_state(
        &self,
        transport: &str,
        id: &str,
        output: Option<&OutputInfo>,
        device: Option<&BluetoothDeviceInfo>,
    ) -> DestState {
        let is_this = |t: &str, i: &str| t == transport && i == id;
        let phase = self
            .snapshot
            .active_output
            .as_ref()
            .filter(|active| is_this(&active.output.transport, &active.output.device_id))
            .map(|active| active.state);
        match phase {
            Some(Some(OutputPhase::Starting)) => return DestState::Connecting,
            Some(Some(OutputPhase::Live) | None) => return DestState::Live,
            // A retry of a failed output shows its own step below.
            Some(Some(OutputPhase::Failed | OutputPhase::Idle)) | None => {}
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
        if phase == Some(Some(OutputPhase::Failed)) {
            return DestState::Failed;
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
            (Some(OutputPhase::Starting), DestState::Connecting),
            (Some(OutputPhase::Live), DestState::Live),
        ] {
            m.snapshot.active_output = Some(active("sonos", "s1", phase));
            // A starting or live output beats an activation and a failure.
            m.activation = activation(ActivationStep::Pairing);
            m.failed_output = Some(("sonos".into(), "s1".into()));
            assert_eq!(state_of(&m, "sonos", "s1"), want);
        }
        // A retry of a failed output shows its own step, then the failure.
        m.snapshot.active_output = Some(active("sonos", "s1", Some(OutputPhase::Failed)));
        m.activation = activation(ActivationStep::Connecting);
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Connecting);
        m.activation = None;
        assert_eq!(state_of(&m, "sonos", "s1"), DestState::Failed);
        m.activation = activation(ActivationStep::Pairing);
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
    fn this_computer_comes_first_and_only_when_present() {
        let mut m = Model::new();
        m.snapshot.bluetooth = Some(vec![]);
        m.snapshot.outputs = vec![output("sink#speaker", "local"), output("s", "sonos")];
        let rows = m.dest_rows();
        assert_eq!(rows[0], DestRow::Header("This computer"));
        assert!(matches!(&rows[1], DestRow::Entry(e) if e.transport == "local"));
        assert_eq!(rows[2], DestRow::Header("Sonos"));
        m.snapshot.outputs.retain(|o| o.transport != "local");
        assert_eq!(m.dest_rows()[0], DestRow::Header("Sonos"));
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
