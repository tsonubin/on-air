export interface StatusResponse {
  status: string;
  version: string;
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

export interface SampleRateResponse {
  sample_rate_hz: number;
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

// Keep in sync with DEFAULT_PORT in packages/core/src/lib.rs
export const DEFAULT_PORT = 47990;

export const API_BASE = `http://127.0.0.1:${DEFAULT_PORT}`;
