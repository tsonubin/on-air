// Hand-maintained mirror of the serde structs in packages/core. Field
// optionality follows serde exactly: a `#[serde(skip_serializing_if =
// "Option::is_none")]` field is optional (`?`) and never `null`; a plain
// `Option<T>` is `T | null`; everything else is required.
//
// Source of truth, by section:
//   packages/api/src/lib.rs            all serde wire structs mirrored below
//   packages/core/src/pipeline/capture.rs  InputBackend
//   packages/core/src/sender/airplay.rs    AirPlayMode

// ---------------------------------------------------------------------------
// Enumerations
// ---------------------------------------------------------------------------

/** Output transports the core can cast to (`session.rs`). `local` is the
 * computer's own speakers and headphone jack (`sender/local.rs`). */
export type Transport = "local" | "sonos" | "airplay" | "bluetooth";

/** How each transport is named in pickers. */
export const TRANSPORT_LABELS: Record<Transport, string> = {
  local: "This computer",
  sonos: "Sonos",
  airplay: "AirPlay",
  bluetooth: "Bluetooth",
};

/** How many speakers an output represents (`session.rs`, `airplay_mdns.rs`). */
export type OutputKind = "solo" | "pair";

/** Loopback capture backend chosen at compile time (`capture::loopback_backend`). */
export type InputBackend =
  | "pipewire-monitor"
  | "coreaudio-screencapturekit"
  | "wasapi-loopback"
  | "cpal-default";

/** AirPlay integration strategy (`airplay::platform_mode`). */
export type AirPlayMode = "avroute-picker" | "owntone";

export type CdAction = "play" | "pause" | "next" | "prev" | "seek" | "goto" | "eject";

// ---------------------------------------------------------------------------
// Error envelope
// ---------------------------------------------------------------------------

/**
 * Error codes the core puts in `ApiErrorBody.code` (`packages/core/src/api/error.rs`).
 * The Rust side chooses snake_case names; the list below is the set known at
 * the time of writing, with the HTTP status each one rides on. The trailing
 * `(string & {})` keeps the union open on purpose: a newer core may send codes
 * this client has never seen, and callers must fall back to a generic message
 * rather than crash. Map `code` to copy; never match on the human-readable
 * `error` text.
 */
export type ApiErrorCode =
  | "invalid_request" // 400
  | "invalid_pin" // 401 from /api/pairing/verify
  | "not_paired" // 401
  | "forbidden" // 403
  | "not_found" // 404
  | "no_active_output" // 404
  | "method_not_allowed" // 405
  | "conflict" // 409: exclusivity / switching conflict
  | "not_ready" // 409: the chosen device or source is not ready (unpaired, disconnected, no disc)
  | "stream_busy" // 503: the Sonos stream is already being served
  | "pin_lockout" // 429
  | "transport_unreachable" // 502
  | "service_paused" // 503
  | "internal" // 500
  | (string & {});

/** Body of every non-2xx response from the core (`Content-Type: application/json`). */
export interface ApiErrorBody {
  /** Human-readable message for logs; not stable, do not match on it. */
  error: string;
  code: ApiErrorCode;
}

// ---------------------------------------------------------------------------
// Status and pairing
// ---------------------------------------------------------------------------

export interface StatusResponse {
  status: string;
  version: string;
  service_enabled: boolean;
  /** Whether a platform sleep inhibitor is held; absent on cores older than 0.2. */
  sleep_inhibited?: boolean;
  /**
   * LAN addresses the core is reachable on, so the desktop can show what the
   * phone must type. Optional on the TypeScript side because cores older than
   * the quality overhaul omit it.
   */
  lan_addresses?: string[];
  firewall_warning?: FirewallWarning;
  /** Requested capture buffer; 0 uses the backend default. */
  capture_buffer_ms?: number;
  /** Reserved: a friendly service name. Not emitted by current cores. */
  name?: string;
}

export interface FirewallWarning {
  subnet: string;
  port: number;
  message: string;
  repair_command: string;
}

export interface AudioBenchmark {
  input_hz: number;
  output_hz: number;
  duration_ms: number;
  pipeline_p95_ms: number;
  scheduler_p95_ms: number;
  worst_case_ms: number;
  recommended_buffer_ms: number;
  within_buffer_budget: boolean;
  /** The DSP quality profile the measured timings can afford. */
  recommended_quality: DspQuality;
}

/**
 * DSP quality profile: selects the resampler on every conversion path.
 * `economy` is the cheap cubic Farrow polynomial interpolator; `balanced`
 * and `high` are windowed sinc interpolation (sinc length 128 / 256).
 */
export type DspQuality = "economy" | "balanced" | "high";

/** `GET /api/audio/quality`. */
export interface AudioQualityResponse {
  /** Current profile, persisted in saved settings. */
  quality: DspQuality;
  /** The last benchmark's verdict; equals `quality` until one has run. */
  recommended: DspQuality;
  /** All selectable profiles. */
  profiles: DspQuality[];
}

/** `PUT /api/audio/quality`; returns 204. */
export interface SetAudioQualityRequest {
  quality: DspQuality;
}

export interface PinResponse {
  pin: string;
}

export interface VerifyRequest {
  pin: string;
}

export interface VerifyResponse {
  token: string;
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

export interface InputsResponse {
  inputs: string[];
}

export interface ActiveInputResponse {
  name: string | null;
  backend: InputBackend;
}

export interface ActivateInputRequest {
  name: string;
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

/**
 * How one output's pipeline stereo signal is fitted to its destination.
 * `auto` is the historical behaviour and never changes what a listener
 * hears; `mono` is a true `(L + R) / 2` downmix into a genuinely mono
 * stream; `stereo` is rejected with an explanation on a mono destination.
 */
export type ChannelMode = "auto" | "stereo" | "mono";

export const CHANNEL_MODE_LABELS: Record<ChannelMode, string> = {
  auto: "Auto",
  stereo: "Stereo",
  mono: "Mono",
};

export interface OutputInfo {
  id: string;
  name: string;
  transport: Transport;
  kind: OutputKind;
  member_count: number;
  needs_pair: boolean;
  paired: boolean;
  /** Saved channel mode; absent means `auto`. */
  channel_mode?: ChannelMode;
  /** Destination channel count when the core can probe it. */
  channels?: number;
}

export interface SetChannelModeRequest {
  transport: Transport;
  device_id: string;
  mode: ChannelMode;
}

export interface OutputsResponse {
  outputs: OutputInfo[];
}

export interface ActiveOutput {
  transport: Transport;
  device_id: string;
  device_name: string;
}

/** Lifecycle of the root session's output (`session::output::OutputPhase`). */
export type OutputPhase = "starting" | "live" | "failed";

/** The active output as `GET /api/outputs/active` reports it. */
export interface ActiveOutputView extends ActiveOutput {
  /** Optional because cores older than the output-session refactor omit it. */
  state?: OutputPhase;
}

/** Phase of the active output; cores that predate the field only report live outputs. */
export function outputPhase(active: Pick<ActiveOutputView, "state">): OutputPhase {
  return active.state ?? "live";
}

/** `GET /api/outputs/active`. */
export interface ActiveOutputResponse {
  active: ActiveOutputView | null;
}

export interface ActivateOutputRequest {
  transport: Transport;
  device_id: string;
}

/** `volume` is a `u8` on the Rust side: send an integer in 0..=100. */
export interface SetVolumeRequest {
  volume: number;
}

export interface OutputVolumeResponse {
  volume: number;
}

// ---------------------------------------------------------------------------
// Cross-protocol sync (roadmap: mixed protocols playing in sync)
// ---------------------------------------------------------------------------

/**
 * Coarse per-output playout sync state (`packages/core/src/sync`).
 * `free-running`: outside any sync group — no delay line, no drift steering;
 * such outputs drift apart from each other, which is fine and expected.
 * `syncing` / `locked`: in a sync group, held to the shared presentation
 * epoch and the delay line; `locked` once drift measurements have settled.
 */
export type SyncState = "free_running" | "syncing" | "locked";

/** One output's sync state as `GET /api/sync` reports it. */
export interface OutputSyncStatus {
  transport: Transport;
  device_id: string;
  device_name: string;
  state: SyncState;
  /** The user's latency trim in milliseconds; may be negative. */
  trim_ms: number;
  /**
   * The documented per-transport default latency in milliseconds. NOT a
   * measurement: the design spec requires acoustic (mic loopback)
   * verification, and none of these figures has been measured on hardware.
   */
  default_latency_ms: number;
  /** `default_latency_ms + trim_ms`, clamped at zero. Still uncalibrated. */
  effective_latency_ms: number;
  /**
   * Whether the latency figure is a calibrated measurement. Never true
   * today: the trim control exists precisely because it is not.
   */
  calibrated: boolean;
  /** Delay line this output's feed holds, in frames; 0 when free-running. */
  delay_frames: number;
  output_sample_rate_hz: number;
  /**
   * Measured consumption drift against the timeline in ppm; positive means
   * the speaker consumes samples faster than nominal. Absent until a feed
   * has measured a window.
   */
  drift_ppm?: number;
  /**
   * The sync group this output belongs to, if any. An output is a member of
   * at most one group at a time.
   */
  group_id?: number;
}

/** One member of a sync group as `GET /api/sync` reports it. */
export interface SyncGroupMember {
  transport: Transport;
  device_id: string;
  /**
   * Filled in when the core knows the display name; join the
   * `GET /api/outputs` catalog instead if it is empty.
   */
  device_name: string;
  /**
   * True while this member is playing together with its group (started at
   * the group's shared presentation epoch).
   */
  playing: boolean;
  /** Why this member is not playing after a start failed, if known. */
  error?: string;
}

/**
 * A named sync group: SEVERAL SPEAKERS PLAYING IN SYNC, not one merged
 * device. Every member keeps its own transport, volume, latency default and
 * trim, and each member is started and stopped on its own.
 */
export interface SyncGroupView {
  id: number;
  /** Display name; names are not unique, `id` identifies the group. */
  name: string;
  members: SyncGroupMember[];
  /** True while at least one member is playing with the group. */
  playing: boolean;
}

/** `GET /api/sync`. */
export interface SyncResponse {
  outputs: OutputSyncStatus[];
  /**
   * Every sync group with its members. Absent on cores older than this
   * feature; tolerate its absence.
   */
  groups?: SyncGroupView[];
}

/**
 * `PUT /api/sync/trim`: manually calibrate one output's latency in
 * milliseconds against what the user actually hears.
 */
export interface SetSyncTrimRequest {
  transport: Transport;
  device_id: string;
  /** -10000..=10000. */
  trim_ms: number;
}

export interface SyncTrimResponse {
  trim_ms: number;
  effective_latency_ms: number;
}

/**
 * `POST /api/sync/groups`: create an empty named sync group. Several named
 * groups may exist at once; names are for display and need not be unique.
 */
export interface CreateSyncGroupRequest {
  /** 1..=64 characters. */
  name: string;
}

/**
 * `POST /api/sync/groups/{id}/members` (add) and
 * `POST /api/sync/groups/{id}/leave` (remove): move one output into or out
 * of a sync group. Membership is configuration: it never disturbs what is
 * playing right now, and it takes effect at the group's next `start`.
 */
export interface SyncGroupMemberRequest {
  transport: Transport;
  device_id: string;
}

// ---------------------------------------------------------------------------
// EQ and sample rate
// ---------------------------------------------------------------------------

export type EqGains = [number, number, number, number, number];

export interface EqResponse {
  gains_db: EqGains;
}

export interface SetEqRequest {
  gains_db: EqGains;
}

export interface SampleRateSide {
  sample_rate_hz: number;
  supported_hz: number[];
  transport?: Transport;
}

export interface SampleRateResponse {
  /** Pipeline / input rate. Kept for older clients. */
  sample_rate_hz: number;
  input: SampleRateSide;
  output: SampleRateSide;
}

/** `sample_rate_hz` is the legacy spelling of `input_hz`; `input_hz` wins when both are set. */
export interface SetSampleRateRequest {
  sample_rate_hz?: number;
  input_hz?: number;
  output_hz?: number;
}

// ---------------------------------------------------------------------------
// AirPlay and Bluetooth
// ---------------------------------------------------------------------------

export interface AirPlayModeResponse {
  mode: AirPlayMode;
}

export interface AirPlayPairRequest {
  device_id: string;
  pin: string;
}

export interface BluetoothDeviceInfo {
  id: string;
  name: string;
  paired: boolean;
  connected: boolean;
}

export interface BluetoothListResponse {
  devices: BluetoothDeviceInfo[];
}

/** Body for `POST /api/bluetooth/pair` and `POST /api/bluetooth/connect`. */
export interface BluetoothIdRequest {
  id: string;
}

// ---------------------------------------------------------------------------
// Audio CD
// ---------------------------------------------------------------------------

export interface CdTrackInfo {
  number: number;
  title?: string;
  duration_ms: number;
}

export interface CdStatus {
  present: boolean;
  playing: boolean;
  track: number;
  track_count: number;
  title?: string;
  album?: string;
  position_ms: number;
  duration_ms: number;
  tracks: CdTrackInfo[];
}

/** `seek` needs `position_ms`, `goto` needs `track`; other actions take neither. */
export interface CdControlRequest {
  action: CdAction;
  position_ms?: number;
  track?: number;
}

export interface CdSimulateTrack {
  title?: string;
  duration_ms?: number;
}

/** `POST /api/mock/cd`; only routed when the core runs in mock mode. */
export interface CdSimulateRequest {
  present: boolean;
  album?: string;
  tracks?: CdSimulateTrack[];
}

// ---------------------------------------------------------------------------
// WebSocket events (`api/ws.rs`, `#[serde(tag = "type")]`)
// ---------------------------------------------------------------------------

/** One channel's RMS and peak level over one processed chunk (full-scale units). */
export interface MeterLevel {
  rms: number;
  peak: number;
}

export type WsEvent =
  /** Sent every 10 s as a text frame so browsers and React Native see liveness. Ignore it. */
  | { type: "Heartbeat" }
  | { type: "OutputStateChanged"; transport: Transport; device_name: string; active: boolean }
  | { type: "InputStateChanged"; name: string | null; active: boolean; error?: string }
  /**
   * One meter reading per channel of the processed stereo stream. A mono
   * capture is duplicated into both sides, so both channels then read the same.
   */
  | { type: "LevelMeter"; left: MeterLevel; right: MeterLevel }
  | { type: "DeviceJoined"; transport: Transport; id: string; name: string }
  | { type: "DeviceLeft"; transport: Transport; id: string }
  | { type: "ServiceStateChanged"; enabled: boolean }
  /**
   * Sync groups changed: one was created, dissolved, its membership moved,
   * or it was started or stopped. Carries no state; re-read `GET /api/sync`.
   */
  | { type: "SyncGroupChanged"; group_id: number }
  | {
      type: "CdStateChanged";
      present: boolean;
      playing: boolean;
      track: number;
      track_count: number;
      title?: string;
      album?: string;
      position_ms: number;
      duration_ms: number;
    };

export type WsEventType = WsEvent["type"];

// ---------------------------------------------------------------------------
// Constants and helpers
// ---------------------------------------------------------------------------

export const AUDIO_CD_INPUT = "Audio CD";

export interface DiscoveredHost {
  host: string;
  port: number;
  version?: string;
  name?: string;
}

// Keep in sync with DEFAULT_PORT in packages/core/src/lib.rs
export const DEFAULT_PORT = 47990;

/** Linux patchbay wire types; mirrored in packages/api/src/lib.rs. */
export type PatchbaySide = "input" | "output";

export interface PatchbayNode {
  id: string;
  name: string;
  side: PatchbaySide;
  kind: string;
  members: string[];
  volume: number;
  muted: boolean;
  is_default: boolean;
  state: string;
  detail: string;
}

export interface PatchbayLink {
  source: string;
  target: string;
  state: string;
  error: string | null;
}

export interface PatchbaySnapshot {
  supported: boolean;
  enabled: boolean;
  nodes: PatchbayNode[];
  links: PatchbayLink[];
  error: string | null;
}

export type PatchbayCommand =
  | { op: "connect" | "disconnect"; source: string; target: string }
  | { op: "combine"; side: PatchbaySide; members: string[] }
  | { op: "remove_group" | "set_default"; id: string }
  | { op: "set_volume"; id: string; volume: number }
  | { op: "set_muted"; id: string; muted: boolean };

/**
 * DNS-SD service type advertised by a running desktop/core.
 *
 * This is the bare `<service>.<proto>` form. Platform APIs differ in what
 * suffix they want: the Rust `mdns-sd` crate needs the fully qualified
 * `_on-air._tcp.local.` (see `packages/core/src/mdns.rs`), while Apple's
 * `NSBonjourServices` plist entry takes `_on-air._tcp` with no domain. Derive
 * those spellings from this constant instead of re-typing the string.
 */
export const MDNS_SERVICE_TYPE = "_on-air._tcp";

/** `MDNS_SERVICE_TYPE` with the `.local.` domain the mdns-sd crate expects. */
export const MDNS_SERVICE_TYPE_FQDN = `${MDNS_SERVICE_TYPE}.local.`;

export const API_BASE = `http://127.0.0.1:${DEFAULT_PORT}`;

/**
 * Build an `http://host:port` base from user input. Strips a leading scheme
 * and trailing slash, and brackets bare IPv6 literals (`fe80::1` becomes
 * `[fe80::1]`) so the result is a valid URL authority.
 */
export function apiBase(host: string, port: number = DEFAULT_PORT): string {
  let trimmed = host
    .trim()
    .replace(/^https?:\/\//i, "")
    .replace(/\/+$/, "");
  // A bare IPv6 literal contains more than one colon and no brackets.
  const isBareIpv6 = !trimmed.startsWith("[") && (trimmed.match(/:/g)?.length ?? 0) >= 2;
  if (isBareIpv6) trimmed = `[${trimmed}]`;
  return `http://${trimmed}:${port}`;
}
