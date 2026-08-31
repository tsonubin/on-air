export interface StatusResponse {
  status: string;
  version: string;
  service_enabled: boolean;
}

export interface InputsResponse {
  inputs: string[];
}

export interface ActiveInputResponse {
  name: string | null;
  backend: string;
}

export interface OutputInfo {
  id: string;
  name: string;
  transport: "sonos" | "airplay" | "bluetooth" | string;
  kind?: "solo" | "pair" | "group" | string;
  member_count?: number;
  needs_pair?: boolean;
  paired?: boolean;
}

export interface OutputsResponse {
  outputs: OutputInfo[];
}

export interface ActiveOutput {
  transport: string;
  device_id: string;
  device_name: string;
}

export interface EqResponse {
  gains_db: [number, number, number, number, number];
}

export interface SampleRateSide {
  sample_rate_hz: number;
  supported_hz: number[];
  transport?: string;
}

export interface SampleRateResponse {
  sample_rate_hz: number;
  input: SampleRateSide;
  output: SampleRateSide;
}

export interface PinResponse {
  pin: string;
}

export interface VerifyResponse {
  token: string;
}

export interface BluetoothDeviceInfo {
  id: string;
  name: string;
  paired: boolean;
  connected: boolean;
}

export interface AirPlayModeResponse {
  mode: "avroute-picker" | "owntone" | string;
}

export type WsEvent =
  | { type: "OutputStateChanged"; transport: string; device_name: string; active: boolean }
  | { type: "LevelMeter"; rms: number; peak: number }
  | { type: "DeviceJoined"; transport: string; id: string; name: string }
  | { type: "DeviceLeft"; transport: string; id: string }
  | { type: "ServiceStateChanged"; enabled: boolean };

export interface DiscoveredHost {
  host: string;
  port: number;
  version?: string;
  name?: string;
}

// Keep in sync with DEFAULT_PORT in packages/core/src/lib.rs
export const DEFAULT_PORT = 47990;

/** DNS-SD type advertised by a running desktop/core (`mdns.rs`). */
export const MDNS_SERVICE_TYPE = "_on-air._tcp";

export const API_BASE = `http://127.0.0.1:${DEFAULT_PORT}`;

export function apiBase(host: string, port: number = DEFAULT_PORT): string {
  const trimmed = host.replace(/^https?:\/\//, "").replace(/\/$/, "");
  return `http://${trimmed}:${port}`;
}
