export type OutputInfo = {
  id: string;
  name: string;
  transport: string;
};

export async function runGoldenPath(apiBase: string): Promise<void> {
  const statusRes = await fetch(`${apiBase}/api/status`);
  const status = (await statusRes.json()) as { status: string };
  if (status.status !== "ok") {
    throw new Error(`status not ok: ${JSON.stringify(status)}`);
  }

  const inputsRes = await fetch(`${apiBase}/api/inputs`);
  const inputs = (await inputsRes.json()) as { inputs: string[] };
  const input = inputs.inputs[0];
  if (!input) throw new Error("no inputs");
  const pick = await fetch(`${apiBase}/api/inputs/active`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name: input }),
  });
  if (!pick.ok) throw new Error(`pick input ${pick.status}`);

  const outputsRes = await fetch(`${apiBase}/api/outputs`);
  const outputs = (await outputsRes.json()) as { outputs: OutputInfo[] };
  const sonos = outputs.outputs.find((o) => o.transport === "sonos");
  if (!sonos) throw new Error("no sonos");
  const activate = await fetch(`${apiBase}/api/outputs/active`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ transport: "sonos", device_id: sonos.id }),
  });
  if (!activate.ok) throw new Error(`activate ${activate.status}`);

  const volume = await fetch(`${apiBase}/api/outputs/active/volume`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ volume: 20 }),
  });
  if (!volume.ok) throw new Error(`volume ${volume.status}`);

  const eq = await fetch(`${apiBase}/api/eq`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ gains_db: [3, 0, 0, 0, -3] }),
  });
  if (!eq.ok) throw new Error(`eq ${eq.status}`);
}

export async function runTransportSwitch(apiBase: string): Promise<string[]> {
  const outputsRes = await fetch(`${apiBase}/api/outputs`);
  const outputs = (await outputsRes.json()) as { outputs: OutputInfo[] };
  const activated: string[] = [];
  for (const transport of ["sonos", "airplay", "bluetooth"]) {
    const device = outputs.outputs.find((o) => o.transport === transport);
    if (!device) throw new Error(`missing ${transport}`);
    const res = await fetch(`${apiBase}/api/outputs/active`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ transport, device_id: device.id }),
    });
    if (!res.ok) throw new Error(`switch ${transport} ${res.status}`);
    activated.push(transport);
  }
  return activated;
}
