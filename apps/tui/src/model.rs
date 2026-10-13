//! The whole UI state in one value. Everything shown comes from the core's
//! snapshot; the rest is focus, modals, one in-flight action and the last
//! error.

use crate::api::{ActiveOutputView, CdStatus, OutputInfo, SampleRateResponse, StatusResponse};
use crate::client::Connection;
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
}

/// A row of the Outputs pane: a transport header or a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRow {
    Header(&'static str),
    /// Index into `Snapshot::outputs`.
    Device(usize),
}

/// Transports in display order with their header labels.
pub const TRANSPORTS: [(&str, &str); 3] = [
    ("sonos", "Sonos"),
    ("airplay", "AirPlay"),
    ("bluetooth", "Bluetooth"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub connection: Connection,
    pub snapshot: Snapshot,
    pub pane: Pane,
    pub source_index: usize,
    /// Index into `output_rows()`; never a header once there is a device.
    pub output_index: usize,
    pub mixer_row: MixerRow,
    pub modal: Modal,
    pub pending: Option<Pending>,
    pub error: Option<String>,
    pub notice: Option<String>,
    /// `None` until the host answers, or where autostart is not ours.
    pub autostart: Option<bool>,
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
            modal: Modal::None,
            pending: None,
            error: None,
            notice: None,
            autostart: None,
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

    /// Outputs grouped under transport headers in `TRANSPORTS` order; a
    /// transport with no devices still gets its header so the pane reads
    /// the same every time.
    pub fn output_rows(&self) -> Vec<OutputRow> {
        let mut rows = Vec::new();
        for (transport, label) in TRANSPORTS {
            rows.push(OutputRow::Header(label));
            for (index, output) in self.snapshot.outputs.iter().enumerate() {
                if output.transport == transport {
                    rows.push(OutputRow::Device(index));
                }
            }
        }
        rows
    }

    /// The device under the Outputs cursor, if the cursor is on one.
    pub fn selected_output(&self) -> Option<&OutputInfo> {
        match self.output_rows().get(self.output_index) {
            Some(OutputRow::Device(index)) => self.snapshot.outputs.get(*index),
            _ => None,
        }
    }
}
