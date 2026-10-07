// Hand-maintained mirror of the serde structs in packages/core. Field
// optionality follows serde exactly: a `#[serde(skip_serializing_if =
// "Option::is_none")]` field is optional (`?`) and never `null`; a plain
// `Option<T>` is `T | null`; everything else is required.
//
// Source of truth, by section:
//   packages/core/src/lib.rs            StatusResponse
//   packages/core/src/api/*.rs          request and response bodies
//   packages/core/src/state.rs          ActiveOutput
//   packages/core/src/cd/mod.rs         CdStatus, CdTrackInfo
//   packages/core/src/pipeline/capture.rs  InputBackend
//   packages/core/src/sender/airplay.rs    AirPlayMode

// ---------------------------------------------------------------------------
// Enumerations
// ---------------------------------------------------------------------------

/** Output transports the core can cast to (`session.rs`). */
export type Transport = "sonos" | "airplay" | "bluetooth";

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
  | "conflict" // 409
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
  /**
   * LAN addresses the core is reachable on, so the desktop can show what the
   * phone must type. Optional on the TypeScript side because cores older than
   * the quality overhaul omit it.
   */
  lan_addresses?: string[];
  /** Reserved: a friendly service name. Not emitted by current cores. */
  name?: string;
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

export interface OutputInfo {
  id: string;
  name: string;
  transport: Transport;
  kind: OutputKind;
  member_count: number;
  needs_pair: boolean;
  paired: boolean;
}

export interface OutputsResponse {
  outputs: OutputInfo[];
}

export interface ActiveOutput {
  transport: Transport;
  device_id: string;
  device_name: string;
}

/** `GET /api/outputs/active`. */
export interface ActiveOutputResponse {
  active: ActiveOutput | null;
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

/** @deprecated Renamed to {@link OutputVolumeResponse} to match the Rust struct. */
export type VolumeResponse = OutputVolumeResponse;

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

export type WsEvent =
  | { type: "OutputStateChanged"; transport: Transport; device_name: string; active: boolean }
  | { type: "LevelMeter"; rms: number; peak: number }
  | { type: "DeviceJoined"; transport: Transport; id: string; name: string }
  | { type: "DeviceLeft"; transport: Transport; id: string }
  | { type: "ServiceStateChanged"; enabled: boolean }
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
