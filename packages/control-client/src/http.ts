import {
  DEFAULT_PORT,
  apiBase,
  type ActiveInputResponse,
  type ActiveOutput,
  type AirPlayModeResponse,
  type BluetoothDeviceInfo,
  type EqResponse,
  type OutputInfo,
  type SampleRateResponse,
  type StatusResponse,
} from "@on-air/api-types";

export { apiBase, DEFAULT_PORT };

export type FetchLike = typeof fetch;

function authHeaders(token?: string, json = false): Record<string, string> {
  const headers: Record<string, string> = {};
  if (json) headers["content-type"] = "application/json";
  if (token) headers.authorization = `Bearer ${token}`;
  return headers;
}

async function request<T>(
  base: string,
  path: string,
  init: RequestInit = {},
  fetchImpl: FetchLike = fetch,
): Promise<T> {
  const response = await fetchImpl(`${base}${path}`, init);
  if (!response.ok) {
    throw new Error(`${path} ${response.status}`);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

export async function fetchStatus(base: string, fetchImpl?: FetchLike): Promise<StatusResponse> {
  return request<StatusResponse>(base, "/api/status", {}, fetchImpl);
}

export async function verifyPin(
  base: string,
  pin: string,
  fetchImpl?: FetchLike,
): Promise<string> {
  const body = await request<{ token: string }>(
    base,
    "/api/pairing/verify",
    {
      method: "POST",
      headers: authHeaders(undefined, true),
      body: JSON.stringify({ pin }),
    },
    fetchImpl,
  );
  return body.token;
}

export async function listInputs(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<string[]> {
  const body = await request<{ inputs: string[] }>(
    base,
    "/api/inputs",
    { headers: authHeaders(token) },
    fetchImpl,
  );
  return body.inputs;
}

export async function getActiveInput(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<ActiveInputResponse> {
  return request<ActiveInputResponse>(
    base,
    "/api/inputs/active",
    { headers: authHeaders(token) },
    fetchImpl,
  );
}

export async function activateInput(
  base: string,
  name: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/inputs/active",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ name }),
    },
    fetchImpl,
  );
}

export async function listOutputs(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<OutputInfo[]> {
  const body = await request<{ outputs: OutputInfo[] }>(
    base,
    "/api/outputs",
    { headers: authHeaders(token) },
    fetchImpl,
  );
  return body.outputs;
}

export async function getActiveOutput(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<ActiveOutput | null> {
  return request<ActiveOutput | null>(
    base,
    "/api/outputs/active",
    { headers: authHeaders(token) },
    fetchImpl,
  );
}

export async function activateOutput(
  base: string,
  transport: string,
  deviceId: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/outputs/active",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ transport, device_id: deviceId }),
    },
    fetchImpl,
  );
}

export async function setVolume(
  base: string,
  volume: number,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/outputs/active/volume",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ volume }),
    },
    fetchImpl,
  );
}

export async function getEq(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<[number, number, number, number, number]> {
  const body = await request<EqResponse>(base, "/api/eq", { headers: authHeaders(token) }, fetchImpl);
  return body.gains_db;
}

export async function setEq(
  base: string,
  gains: [number, number, number, number, number],
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/eq",
    {
      method: "PUT",
      headers: authHeaders(token, true),
      body: JSON.stringify({ gains_db: gains }),
    },
    fetchImpl,
  );
}

export async function getSampleRate(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<SampleRateResponse> {
  return request<SampleRateResponse>(
    base,
    "/api/sample-rate",
    { headers: authHeaders(token) },
    fetchImpl,
  );
}

export async function setSampleRate(
  base: string,
  body: { input_hz?: number; output_hz?: number; sample_rate_hz?: number },
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/sample-rate",
    {
      method: "PUT",
      headers: authHeaders(token, true),
      body: JSON.stringify(body),
    },
    fetchImpl,
  );
}

export async function getAirplayMode(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<string> {
  const body = await request<AirPlayModeResponse>(
    base,
    "/api/airplay/mode",
    { headers: authHeaders(token) },
    fetchImpl,
  );
  return body.mode;
}

export async function pairAirplay(
  base: string,
  deviceId: string,
  pin: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/airplay/pair",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ device_id: deviceId, pin }),
    },
    fetchImpl,
  );
}

export async function listBluetooth(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<BluetoothDeviceInfo[]> {
  const body = await request<{ devices?: BluetoothDeviceInfo[] } | BluetoothDeviceInfo[]>(
    base,
    "/api/bluetooth/devices",
    { headers: authHeaders(token) },
    fetchImpl,
  );
  return Array.isArray(body) ? body : (body.devices ?? []);
}

export async function pairBluetooth(
  base: string,
  id: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/bluetooth/pair",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ id }),
    },
    fetchImpl,
  );
}

export async function connectBluetooth(
  base: string,
  id: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  await request(
    base,
    "/api/bluetooth/connect",
    {
      method: "POST",
      headers: authHeaders(token, true),
      body: JSON.stringify({ id }),
    },
    fetchImpl,
  );
}

export function wsUrl(base: string, token?: string): string {
  const ws = base.replace(/^http/i, "ws");
  if (!token) return `${ws}/api/ws`;
  return `${ws}/api/ws?token=${encodeURIComponent(token)}`;
}

export function prettyInput(name: string): string {
  if (name.startsWith("Discard all samples")) return "Null device";
  if (name.includes("PipeWire Sound Server")) return "PipeWire";
  if (name.startsWith("Default ALSA")) return "Default";
  if (name.includes("analog") && name.endsWith(".monitor")) return "Analog monitor";
  if (name.endsWith(".monitor")) return "System monitor";
  if (name.includes("CS4208 Analog")) return "Built-in analog";
  if (name.includes("HDMI")) return name.replace("HDA Intel HDMI, ", "HDMI ");
  return name;
}

export async function goldenPathSonos(base: string, pin: string, fetchImpl?: FetchLike): Promise<void> {
  await fetchStatus(base, fetchImpl);
  const token = await verifyPin(base, pin, fetchImpl);
  const inputs = await listInputs(base, token, fetchImpl);
  if (inputs[0]) await activateInput(base, inputs[0], token, fetchImpl);
  const outputs = await listOutputs(base, token, fetchImpl);
  const sonos = outputs.find((o) => o.transport === "sonos");
  if (!sonos) throw new Error("no sonos output");
  await activateOutput(base, "sonos", sonos.id, token, fetchImpl);
  await setVolume(base, 20, token, fetchImpl);
  await setEq(base, [3, 0, 0, 0, -3], token, fetchImpl);
}

export async function switchTransports(base: string, token?: string, fetchImpl?: FetchLike): Promise<string[]> {
  const outputs = await listOutputs(base, token, fetchImpl);
  const order = ["sonos", "airplay", "bluetooth"] as const;
  const activated: string[] = [];
  for (const transport of order) {
    const device = outputs.find((o) => o.transport === transport);
    if (!device) throw new Error(`missing ${transport}`);
    await activateOutput(base, transport, device.id, token, fetchImpl);
    activated.push(transport);
  }
  return activated;
}
