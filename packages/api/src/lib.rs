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
    /// The DSP quality profile the measured timings can afford. Not an
    /// estimate of what the machine "should" do — the same measured worst
    /// case the buffer rule uses, budgeted with the same safety factor.
    pub recommended_quality: DspQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetCaptureBufferRequest {
    pub buffer_ms: u32,
}

/// DSP quality profile: selects the resampler on every conversion path.
///
/// - `economy`: the cubic Farrow polynomial interpolator. Cheapest; this
///   is what every on-air resampler used before quality profiles existed.
///   ~0.03 ms per 1024-frame stereo chunk (44.1→48 kHz, release, dev host).
/// - `balanced`: windowed sinc interpolation, sinc length 128. ~0.5 ms per
///   chunk on the same run — roughly 20× economy, an order of magnitude
///   that matters on the old hardware on-air targets.
/// - `high`: windowed sinc interpolation, sinc length 256. ~0.8 ms per
///   chunk — about 1.5× balanced for the longer filter.
///
/// Those are one machine's measurements (`dsp_resample.rs`'s
/// `report_per_profile_resampler_costs`), not promises: the audio check
/// measures each profile on the machine that will run it and
/// [`AudioBenchmark::recommended_quality`] is decided from that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DspQuality {
    Economy,
    Balanced,
    High,
}

impl DspQuality {
    /// Every selectable profile, cheapest first.
    pub const ALL: [DspQuality; 3] = [DspQuality::Economy, DspQuality::Balanced, DspQuality::High];
}

impl Default for DspQuality {
    /// Economy: a machine that never runs the audio check pays no more CPU
    /// than it did before quality profiles existed.
    fn default() -> Self {
        DspQuality::Economy
    }
}

/// `GET /api/audio/quality`: the chosen profile, the last benchmark's
/// recommendation, and every profile a client may offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioQualityResponse {
    /// Current profile, persisted in saved settings.
    pub quality: DspQuality,
    /// The last `POST /api/audio/benchmark`'s verdict; equals `quality`
    /// until a benchmark has run.
    pub recommended: DspQuality,
    /// All selectable profiles.
    pub profiles: Vec<DspQuality>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAudioQualityRequest {
    pub quality: DspQuality,
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

/// How one output's pipeline stereo signal is fitted to its destination.
/// Saved per output; the default, `auto`, is the historical behaviour and
/// never changes what a listener hears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMode {
    /// Fit the destination's reported channel count (the historical fit:
    /// mono gets a `(L + R) / 2` downmix, two channels pass through, wider
    /// destinations keep the extra channels silent).
    #[default]
    Auto,
    /// Always stereo. Rejected with an explanation on a mono destination,
    /// never silently collapsed.
    Stereo,
    /// A true `(L + R) / 2` downmix: the stream becomes genuinely mono.
    Mono,
}

impl ChannelMode {
    /// All modes, in the order the clients cycle them.
    pub const ALL: [ChannelMode; 3] = [ChannelMode::Auto, ChannelMode::Stereo, ChannelMode::Mono];
}

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
    /// The saved channel mode of this output; absent means [`ChannelMode::Auto`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_mode: Option<ChannelMode>,
    /// What the destination reports for its channel count, when the core
    /// can probe it. Absent means "unknown"; the sink re-checks at open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,
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

/// `PUT /api/outputs/channel-mode`: one output's channel mode. Rejected
/// with an explanation when the mode cannot fit the destination (stereo on
/// a mono device).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetChannelModeRequest {
    pub transport: String,
    pub device_id: String,
    pub mode: ChannelMode,
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
// Cross-protocol sync (roadmap: mixed protocols playing in sync)
// ---------------------------------------------------------------------------

/// Coarse per-output playout sync state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Playing outside any sync group: no delay line and no drift steering.
    /// Such outputs free-run and drift apart from each other, which is fine
    /// and expected.
    #[default]
    FreeRunning,
    /// In a sync group: the delay line is applied and drift is still being
    /// measured and steered.
    Syncing,
    /// In a sync group with settled drift measurements.
    Locked,
}

/// One output's sync state as `GET /api/sync` reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSyncStatus {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
    pub state: SyncState,
    /// The user's latency trim in milliseconds, which may be negative.
    pub trim_ms: i32,
    /// The documented per-transport default latency in milliseconds: the
    /// guess used where no measurement has been applied. NOT a measurement
    /// itself — `calibration` / `calibrated` say whether the figure in
    /// effect is one.
    pub default_latency_ms: u32,
    /// The latency the sync machinery uses: the measured value when one has
    /// been applied, else `default_latency_ms`, plus `trim_ms`, clamped at
    /// zero. Only `calibrated` says which base is in effect — a client must
    /// never present a default as a measurement.
    pub effective_latency_ms: u32,
    /// Whether the latency figure is based on an acoustic measurement
    /// (`calibration` is present). False for documented defaults and for
    /// user trims: a trim corrects by ear and is not a measurement.
    pub calibrated: bool,
    /// The acoustic measurement backing this row, when one has been
    /// applied. Present exactly when `calibrated` is true; absent rows are
    /// defaults and say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<CalibrationMeasurement>,
    /// The delay line this output's feed holds, in frames at
    /// `output_sample_rate_hz` (0 when free-running).
    pub delay_frames: u64,
    pub output_sample_rate_hz: u32,
    /// Measured consumption drift against the timeline, parts per million:
    /// positive means the speaker consumes samples faster than nominal.
    /// Absent until a feed has measured a window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_ppm: Option<f64>,
    /// The sync group this output belongs to, if any. An output is a member
    /// of at most one group at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u64>,
}

/// One member of a sync group as `GET /api/sync` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncGroupMember {
    pub transport: String,
    pub device_id: String,
    /// Filled in when the core knows the display name; clients may join the
    /// `GET /api/outputs` catalog instead.
    pub device_name: String,
    /// True while this member is playing together with its group (started
    /// at the group's shared presentation epoch).
    pub playing: bool,
    /// Why this member is not playing after a start failed, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A named sync group: SEVERAL SPEAKERS PLAYING IN SYNC, not one merged
/// device. Every member keeps its own transport, volume, latency default
/// and trim, and each member is started and stopped on its own session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncGroupView {
    pub id: u64,
    /// Display name chosen on creation. Names are not unique; the `id`
    /// identifies the group.
    pub name: String,
    pub members: Vec<SyncGroupMember>,
    /// True while at least one member is playing with the group.
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub outputs: Vec<OutputSyncStatus>,
    /// Every sync group with its members. Absent on cores older than this
    /// feature; clients must tolerate its absence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<SyncGroupView>>,
}

/// `PUT /api/sync/trim`: manually calibrate one output's latency in
/// milliseconds against what the user actually hears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSyncTrimRequest {
    pub transport: String,
    pub device_id: String,
    /// -10000..=10000.
    pub trim_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTrimResponse {
    pub trim_ms: i32,
    pub effective_latency_ms: u32,
}

/// `POST /api/sync/groups`: create an empty named sync group. Several named
/// groups may exist at once; names are for display and need not be unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSyncGroupRequest {
    /// 1..=64 characters.
    pub name: String,
}

/// `POST /api/sync/groups/{id}/members` (add) and
/// `POST /api/sync/groups/{id}/leave` (remove): move one output into or out
/// of a sync group. Membership is configuration: it never disturbs what is
/// playing right now, and it takes effect at the group's next `start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncGroupMemberRequest {
    pub transport: String,
    pub device_id: String,
}

// ---------------------------------------------------------------------------
// Acoustic latency calibration (roadmap B10)
// ---------------------------------------------------------------------------

/// How trustworthy a completed latency measurement is. The figure is only
/// ever applied to the sync machinery when it is [`Self::High`] or
/// [`Self::Medium`]; a [`Self::Low`] result is reported as bad and never
/// replaces a default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationConfidence {
    /// Every run found a clean, consistent arrival.
    High,
    /// Usable: a clear arrival, but one or more runs were marginal or the
    /// runs disagreed slightly.
    #[default]
    Medium,
    /// Not trustworthy: the correlation peak was weak or the runs
    /// disagreed badly. Reported, never applied.
    Low,
}

/// One completed acoustic latency measurement for one output: how long
/// audio takes to travel from the pipeline PCM bus, through the transport,
/// out of the speaker and back into the microphone (the mic loopback).
///
/// The figure is a LOOPBACK measurement. Beyond the transport's own
/// buffering it includes the speaker-to-microphone propagation (~3 ms per
/// metre) and the capture path's delivery latency (bounded by the capture
/// buffer the run reports). Those two terms are constant for every
/// transport measured on one machine in one position, so the *differences*
/// between transports — what a sync group aligns — are unaffected by them;
/// any residual absolute offset is what the manual trim is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationMeasurement {
    pub transport: String,
    pub device_id: String,
    /// Measured latency in microseconds (sub-millisecond resolution; the
    /// estimator's own floor is far below one millisecond).
    pub latency_us: u64,
    /// How trustworthy the figure is. Only `high` and `medium` are applied
    /// to the sync machinery.
    pub confidence: CalibrationConfidence,
    /// Probe runs whose detection was trusted and folded into `latency_us`
    /// (the median of them).
    pub runs: u32,
    /// Capture samples analysed across those runs.
    pub samples: u64,
    /// Highest minus lowest accepted run, in microseconds: how stable the
    /// number is when repeated.
    pub spread_us: u64,
    /// The capture rate the delay was measured at.
    pub sample_rate_hz: u32,
    /// Unix seconds when the run completed.
    pub measured_at_unix_s: u64,
    /// Each accepted run's measured delay in microseconds, sorted
    /// ascending (the median is `latency_us`). Re-running and comparing
    /// these shows whether the number is stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_delays_us: Vec<u64>,
}

/// One target's progress inside a calibration run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationTargetState {
    #[default]
    Pending,
    Running,
    /// Measured and applied (confidence `high` or `medium`).
    Complete,
    /// Not applied: the target errored, or the measurement was too
    /// untrustworthy to replace the default.
    Failed,
}

/// One output's state inside a calibration run as `GET
/// /api/sync/calibration` reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationTargetStatus {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
    pub state: CalibrationTargetState,
    /// What was measured, including low-confidence attempts that were not
    /// applied. Absent until the target has finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement: Option<CalibrationMeasurement>,
    /// Why this target produced no applied measurement, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A calibration run's overall state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationRunState {
    /// Every target has not yet finished.
    #[default]
    Running,
    /// Finished; at least one target produced an applied measurement.
    Complete,
    /// Finished; no target produced an applied measurement.
    Failed,
}

/// A calibration run as `GET /api/sync/calibration` reports it. Targets are
/// measured one after another (transports are exclusive), in request order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationRunView {
    pub run_id: u64,
    /// `running` until every target has finished; then `complete` when at
    /// least one target produced an applied measurement, `failed` when none
    /// did.
    pub state: CalibrationRunState,
    pub targets: Vec<CalibrationTargetStatus>,
    /// Unix seconds when the run started.
    pub started_at_unix_s: u64,
}

/// `POST /api/sync/calibrate`: start an acoustic calibration run (roadmap
/// B10). The core plays a known probe through each target transport in
/// turn, listens on the given microphone, and measures the delay by
/// cross-correlation. The reply is immediate; the run continues in the
/// background and is monitored with `GET /api/sync/calibration`.
///
/// Side effects, deliberately: outputs are exclusive, so the run activates
/// each target (stopping whatever output was active) and stops it again
/// afterwards; the input session is suspended while probes play — one
/// producer per PCM bus — and restored afterwards. The core must be
/// unpaused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartCalibrationRequest {
    /// 1..=16 targets, measured one after another. "All transports" is the
    /// client sending one target per output.
    pub targets: Vec<CalibrationTarget>,
    /// Microphone to listen on: the label `GET /api/inputs` lists. A
    /// loopback/monitor source is refused — measuring a transport against
    /// its own digital monitor would report ~0 and be a lie.
    pub capture_input: String,
    /// Probe repetitions per target (1..=10). More runs cost time and buy
    /// stability; 3 is the default.
    #[serde(default)]
    pub runs_per_target: Option<u32>,
    /// Longest playout delay searched for, per target, in milliseconds
    /// (500..=15000). Defaults per transport (AirPlay searches the
    /// longest).
    #[serde(default)]
    pub max_delay_ms: Option<u32>,
}

/// One output a calibration run will measure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationTarget {
    pub transport: String,
    pub device_id: String,
}

/// `GET /api/sync/calibration`: the run in progress, if any, and the most
/// recently finished run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationStatusResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<CalibrationRunView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<CalibrationRunView>,
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

/// One channel's RMS and peak level over one processed chunk. `rms` and
/// `peak` are in full-scale units (`1.0` = digital full scale).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeterLevel {
    pub rms: f32,
    pub peak: f32,
}

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
    /// One meter reading per channel of the processed stereo stream. A mono
    /// capture is duplicated into both sides upstream, so both channels
    /// then read the same.
    LevelMeter {
        left: MeterLevel,
        right: MeterLevel,
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
    /// Sync groups changed: one was created, dissolved, its membership
    /// moved, or it was started or stopped. The event carries no state;
    /// clients re-read `GET /api/sync`.
    SyncGroupChanged {
        group_id: u64,
    },
    /// A calibration run started, made progress, finished or failed. The
    /// event carries no state; clients re-read `GET /api/sync/calibration`
    /// and `GET /api/sync`.
    CalibrationChanged {
        run_id: u64,
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
    fn level_meter_carries_one_meter_level_per_channel() {
        let json = serde_json::to_value(WsEvent::LevelMeter {
            left: MeterLevel {
                rms: 0.25,
                peak: 0.5,
            },
            right: MeterLevel {
                rms: 0.0,
                peak: 0.0,
            },
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "LevelMeter",
                "left": {"rms": 0.25, "peak": 0.5},
                "right": {"rms": 0.0, "peak": 0.0},
            })
        );
    }

    #[test]
    fn dsp_quality_serializes_as_snake_case() {
        for (quality, wire) in [
            (DspQuality::Economy, "economy"),
            (DspQuality::Balanced, "balanced"),
            (DspQuality::High, "high"),
        ] {
            let json = serde_json::to_value(quality).unwrap();
            assert_eq!(json, wire);
            assert_eq!(serde_json::from_value::<DspQuality>(json).unwrap(), quality);
        }
    }

    #[test]
    fn active_output_view_flattens_the_identity() {
        let view = ActiveOutputView {
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
