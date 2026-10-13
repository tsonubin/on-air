//! Hand-maintained mirror of the core's JSON shapes, named after the
//! TypeScript interfaces in `packages/api-types/src/index.ts` and checked
//! against the real serde structs by `tests/mock_core.rs`. Optionality
//! follows serde: a `skip_serializing_if` field is `Option` and never
//! `null`; a plain `Option<T>` is `T | null`; everything else is required.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub service_enabled: bool,
    /// Cores older than the Omarchy-first change omit it.
    #[serde(default)]
    pub sleep_inhibited: Option<bool>,
    /// Cores older than the quality overhaul omit it.
    #[serde(default)]
    pub lan_addresses: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinResponse {
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveInputResponse {
    pub name: Option<String>,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub kind: String,
    pub member_count: u32,
    pub needs_pair: bool,
    pub paired: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveOutput {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
}

/// Lifecycle of the exclusive output (`session::output::OutputPhase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputPhase {
    Starting,
    Live,
    Failed,
}

/// The active output as `GET /api/outputs/active` reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveOutputView {
    #[serde(flatten)]
    pub output: ActiveOutput,
    /// Cores older than the output-session refactor omit it.
    #[serde(default)]
    pub state: Option<OutputPhase>,
}

impl ActiveOutputView {
    pub fn is_live(&self) -> bool {
        matches!(self.state, Some(OutputPhase::Live) | None)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveOutputResponse {
    pub active: Option<ActiveOutputView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputVolumeResponse {
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqResponse {
    pub gains_db: [f32; 5],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleRateSide {
    pub sample_rate_hz: u32,
    pub supported_hz: Vec<u32>,
    #[serde(default)]
    pub transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleRateResponse {
    /// Pipeline / input rate, kept for older clients.
    pub sample_rate_hz: u32,
    pub input: SampleRateSide,
    pub output: SampleRateSide,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirPlayModeResponse {
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BluetoothDeviceInfo {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BluetoothListResponse {
    pub devices: Vec<BluetoothDeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdTrackInfo {
    pub number: u8,
    #[serde(default)]
    pub title: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdStatus {
    pub present: bool,
    pub playing: bool,
    pub track: u8,
    pub track_count: u8,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub tracks: Vec<CdTrackInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdAction {
    Play,
    Pause,
    Next,
    Prev,
    Seek,
    Goto,
    Eject,
}

/// Body of every non-2xx response from the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceResponse {
    pub enabled: bool,
}

/// WebSocket events (`api/ws.rs`, `#[serde(tag = "type")]`). A `type` this
/// client has never seen parses as `Unknown` and is ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    Heartbeat,
    OutputStateChanged {
        transport: String,
        device_name: String,
        active: bool,
    },
    InputStateChanged {
        name: Option<String>,
        active: bool,
        #[serde(default)]
        error: Option<String>,
    },
    LevelMeter {
        rms: f32,
        peak: f32,
    },
    DeviceJoined {
        transport: String,
        id: String,
        name: String,
    },
    DeviceLeft {
        transport: String,
        id: String,
    },
    ServiceStateChanged {
        enabled: bool,
    },
    CdStateChanged {
        present: bool,
        playing: bool,
        track: u8,
        track_count: u8,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        album: Option<String>,
        position_ms: u64,
        duration_ms: u64,
    },
    #[serde(other)]
    Unknown,
}
