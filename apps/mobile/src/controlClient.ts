import { DEFAULT_PORT, type OutputInfo, type StatusResponse } from "@on-air/api-types";

export function apiBase(host: string, port: number = DEFAULT_PORT): string {
  return `http://${host}:${port}`;
}

export async function fetchStatus(base: string): Promise<StatusResponse> {
  const response = await fetch(`${base}/api/status`);
  if (!response.ok) throw new Error(`status ${response.status}`);
  return response.json();
}

export async function verifyPin(base: string, pin: string): Promise<string> {
  const response = await fetch(`${base}/api/pairing/verify`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ pin }),
  });
  if (!response.ok) throw new Error(`pairing ${response.status}`);
  const body = (await response.json()) as { token: string };
  return body.token;
}

export async function listOutputs(base: string, token?: string): Promise<OutputInfo[]> {
  const headers: Record<string, string> = {};
  if (token) headers.authorization = `Bearer ${token}`;
  const response = await fetch(`${base}/api/outputs`, { headers });
  if (!response.ok) throw new Error(`outputs ${response.status}`);
  const body = (await response.json()) as { outputs: OutputInfo[] };
  return body.outputs;
}

export async function activateOutput(
  base: string,
  transport: string,
  deviceId: string,
): Promise<void> {
  const response = await fetch(`${base}/api/outputs/active`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ transport, device_id: deviceId }),
  });
  if (!response.ok) throw new Error(`activate ${response.status}`);
}

export async function setVolume(base: string, volume: number): Promise<void> {
  const response = await fetch(`${base}/api/outputs/active/volume`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ volume }),
  });
  if (!response.ok) throw new Error(`volume ${response.status}`);
}

export async function setEq(
  base: string,
  gains: [number, number, number, number, number],
): Promise<void> {
  const response = await fetch(`${base}/api/eq`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ gains_db: gains }),
  });
  if (!response.ok) throw new Error(`eq ${response.status}`);
}

export async function goldenPathSonos(base: string, pin: string): Promise<void> {
  await fetchStatus(base);
  await verifyPin(base, pin);
  const outputs = await listOutputs(base);
  const sonos = outputs.find((o) => o.transport === "sonos");
  if (!sonos) throw new Error("no sonos output");
  await activateOutput(base, "sonos", sonos.id);
  await setVolume(base, 20);
  await setEq(base, [3, 0, 0, 0, -3]);
}

export async function switchTransports(base: string): Promise<string[]> {
  const outputs = await listOutputs(base);
  const order = ["sonos", "airplay", "bluetooth"] as const;
  const activated: string[] = [];
  for (const transport of order) {
    const device = outputs.find((o) => o.transport === transport);
    if (!device) throw new Error(`missing ${transport}`);
    await activateOutput(base, transport, device.id);
    activated.push(transport);
  }
  return activated;
}
