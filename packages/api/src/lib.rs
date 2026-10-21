//! The control plane's wire types, shared by the core (which serializes
//! them) and the Rust clients (which deserialize them). The TypeScript
//! mirror is `packages/api-types/src/index.ts`; keep the two in step.
//!
//! Optionality follows one rule on both sides: a field a client may not
//! receive from an older core is `Option` with `#[serde(default)]` and is
//! omitted from the JSON when `None`.

use serde::{Deserialize, Serialize};

/// Port the core listens on; the phone probes it across the LAN.
pub const DEFAULT_PORT: u16 = 47990;

/// Name of the Audio CD input in `inputs` lists and events.
pub const AUDIO_CD_INPUT: &str = "Audio CD";

/// The transports an output can belong to, as they appear in JSON.
pub mod transport {
    pub const SONOS: &str = "sonos";
    pub const AIRPLAY: &str = "airplay";
    pub const BLUETOOTH: &str = "bluetooth";
    pub const ALL: [&str; 3] = [SONOS, AIRPLAY, BLUETOOTH];
}

/// Stable `code` values of the error envelope. Clients branch on these,
/// never on the `error` text.
pub mod codes {
    pub const INVALID_REQUEST: &str = "invalid_request";
    pub const INVALID_PIN: &str = "invalid_pin";
    pub const NOT_PAIRED: &str = "not_paired";
    pub const FORBIDDEN: &str = "forbidden";
    pub const NOT_FOUND: &str = "not_found";
    pub const NO_ACTIVE_OUTPUT: &str = "no_active_output";
    pub const METHOD_NOT_ALLOWED: &str = "method_not_allowed";
    pub const CONFLICT: &str = "conflict";
    pub const NOT_READY: &str = "not_ready";
    pub const STREAM_BUSY: &str = "stream_busy";
    pub const PIN_LOCKOUT: &str = "pin_lockout";
    pub const TRANSPORT_UNREACHABLE: &str = "transport_unreachable";
    pub const SERVICE_PAUSED: &str = "service_paused";
    pub const INTERNAL: &str = "internal";
}

/// Body of every non-2xx response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    /// For logs; not stable.
    pub error: String,
    /// One of [`codes`].
    pub code: String,
}

// ---------------------------------------------------------------------------
// Linux patchbay (additive; the original exclusive-session API is unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchbaySide {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchbayNode {
    pub id: String,
    pub name: String,
    pub side: PatchbaySide,
    pub kind: String,
    pub members: Vec<String>,
    pub volume: u8,
    pub muted: bool,
    pub is_default: bool,
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchbayLink {
    pub source: String,
    pub target: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchbaySnapshot {
    pub supported: bool,
    pub enabled: bool,
    pub nodes: Vec<PatchbayNode>,
    pub links: Vec<PatchbayLink>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatchbayCommand {
    Connect {
        source: String,
        target: String,
    },
    Disconnect {
        source: String,
        target: String,
    },
    Combine {
        side: PatchbaySide,
        members: Vec<String>,
    },
    RemoveGroup {
        id: String,
    },
    SetDefault {
        id: String,
    },
    SetVolume {
        id: String,
        volume: u8,
    },
    SetMuted {
        id: String,
        muted: bool,
    },
}

// ---------------------------------------------------------------------------
// Status, pairing, service
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub service_enabled: bool,
    /// Whether a platform sleep inhibitor is held right now. Absent on
    /// cores older than 0.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_inhibited: Option<bool>,
    /// Non-loopback IPv4 addresses of the host, so a desktop can show what
    /// a phone must type. Absent on cores older than 0.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_addresses: Option<Vec<String>>,
    /// A recent, observed host firewall block affecting the audio port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firewall_warning: Option<FirewallWarning>,
    /// Zero means the capture backend's default buffer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_buffer_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBenchmark {
    pub input_hz: u32,
    pub output_hz: u32,
    pub duration_ms: u64,
    pub pipeline_p95_ms: f64,
    pub scheduler_p95_ms: f64,
    pub worst_case_ms: f64,
    pub recommended_buffer_ms: u32,
    pub within_buffer_budget: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetCaptureBufferRequest {
    pub buffer_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirewallWarning {
    pub subnet: String,
    pub port: u16,
    pub message: String,
    pub repair_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinResponse {
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetServiceRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceResponse {
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveInputResponse {
    pub name: Option<String>,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateInputRequest {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: String,
    pub name: String,
    /// One of [`transport`].
    pub transport: String,
    /// `"solo"` or `"pair"` (a bonded stereo pair; cast to the coordinator).
    pub kind: String,
    pub member_count: u8,
    pub needs_pair: bool,
    pub paired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputsResponse {
    pub outputs: Vec<OutputInfo>,
}

/// Which speaker the exclusive output belongs to. Also persisted in the
/// core's saved settings, so its shape is part of the settings file format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveOutput {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateOutputRequest {
    pub transport: String,
    pub device_id: String,
}

/// Lifecycle of the exclusive output as clients see it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputPhase {
    /// Nothing is active. Never reported inside an `ActiveOutputView`.
    #[default]
    Idle,
    /// `start()` is in flight. The identity is already visible because Sonos
    /// pulls the radio URI during Play.
    Starting,
    Live,
    /// Start (or a rollback) failed and the sender's cleanup also failed.
    /// Nothing else may start until a later stop succeeds.
    Failed,
}

/// The active output as `GET /api/outputs/active` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveOutputView {
    #[serde(flatten)]
    pub output: ActiveOutput,
    /// Absent on cores older than 0.2, which only reported live outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<OutputPhase>,
    /// Failure of the retained sender, shared by all control surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ActiveOutputView {
    /// Live, or reported by a core too old to say.
    pub fn is_live(&self) -> bool {
        matches!(self.state, Some(OutputPhase::Live) | None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveOutputResponse {
    pub active: Option<ActiveOutputView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetVolumeRequest {
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputVolumeResponse {
    pub volume: u8,
}

// ---------------------------------------------------------------------------
// EQ and sample rates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqResponse {
    pub gains_db: [f32; 5],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetEqRequest {
    pub gains_db: [f32; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRateSide {
    pub sample_rate_hz: u32,
    pub supported_hz: Vec<u32>,
    /// The output side names the transport the catalog belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRateResponse {
    /// Pipeline / input rate. Kept for older clients.
    pub sample_rate_hz: u32,
    pub input: SampleRateSide,
    pub output: SampleRateSide,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSampleRateRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate_hz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hz: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hz: Option<u32>,
}

// ---------------------------------------------------------------------------
// AirPlay and Bluetooth
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirPlayModeResponse {
    /// `"owntone"`, `"pyatv"` or `"avroute-picker"`.
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirPlayPairRequest {
    pub device_id: String,
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothDeviceInfo {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothListResponse {
    pub devices: Vec<BluetoothDeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothIdRequest {
    pub id: String,
}

// ---------------------------------------------------------------------------
// Audio CD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdTrackInfo {
    pub number: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdStatus {
    pub present: bool,
    pub playing: bool,
    pub track: u8,
    pub track_count: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub tracks: Vec<CdTrackInfo>,
}

impl CdStatus {
    /// No disc.
    pub fn empty() -> Self {
        CdStatus {
            present: false,
            playing: false,
            track: 0,
            track_count: 0,
            title: None,
            album: None,
            position_ms: 0,
            duration_ms: 0,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdAction {
    Play,
    Pause,
    Next,
    Prev,
    Seek,
    Goto,
    Eject,
}

/// `seek` needs `position_ms`, `goto` needs `track`; other actions take neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdControlRequest {
    pub action: CdAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<u8>,
}

// ---------------------------------------------------------------------------
// WebSocket events
// ---------------------------------------------------------------------------

/// Events on `/api/ws`, tagged by `type`. A `type` a client has never seen
/// parses as [`WsEvent::Unknown`] and is ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    OutputStateChanged {
        transport: String,
        device_name: String,
        active: bool,
    },
    /// The live input changed. `active: false` with an `error` means the
    /// capture or processing thread died and the input session cleared
    /// itself.
    InputStateChanged {
        name: Option<String>,
        active: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        album: Option<String>,
        position_ms: u64,
        duration_ms: u64,
    },
    /// Keep-alive text frame the core sends per socket every 10 s; never
    /// broadcast. Clients only reset their stall timer on it.
    Heartbeat,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_fields_are_omitted_when_none_and_tolerated_when_missing() {
        let status = StatusResponse {
            status: "ok".into(),
            version: "0.2.0".into(),
            service_enabled: true,
            sleep_inhibited: None,
            lan_addresses: None,
            firewall_warning: None,
            capture_buffer_ms: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"status": "ok", "version": "0.2.0", "service_enabled": true})
        );
        let back: StatusResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back, status);
    }

    #[test]
    fn an_unknown_event_type_parses_as_unknown() {
        let ev: WsEvent = serde_json::from_str(r#"{"type":"Brand New","x":1}"#).unwrap();
        assert_eq!(ev, WsEvent::Unknown);
        let ev: WsEvent = serde_json::from_str(r#"{"type":"Heartbeat"}"#).unwrap();
        assert_eq!(ev, WsEvent::Heartbeat);
    }

    #[test]
    fn active_output_view_flattens_the_identity() {
        let view = ActiveOutputView {
            error: None,
            output: ActiveOutput {
                transport: transport::SONOS.into(),
                device_id: "uuid:1".into(),
                device_name: "Kitchen".into(),
            },
            state: Some(OutputPhase::Starting),
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["transport"], "sonos");
        assert_eq!(json["state"], "starting");
        assert!(!view.is_live());
    }
}
