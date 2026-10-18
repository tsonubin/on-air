// Shared end-to-end scenarios run against a mock core by the desktop and
// mobile e2e suites. They live here, not in the production control client,
// so the client ships no test choreography. Import the client the way the
// other e2e files do: by relative path, so no build step is needed.
import type {
  OutputSyncStatus,
  SyncGroupView,
  Transport,
} from "@on-air/api-types";
import {
  activateInput,
  activateOutput,
  fetchStatus,
  getSync,
  listInputs,
  listOutputs,
  type RequestOptions,
  setEq,
  setSyncTrim,
  setVolume,
  addSyncGroupMember,
  createSyncGroup,
  startSyncGroup,
} from "../../packages/control-client/src/index.ts";

/**
 * Pick the first input, go live on the first Sonos output, then set volume
 * and EQ. `token` comes from `verifyPin` (or is `undefined` when the core
 * allows loopback callers without pairing).
 */
export async function goldenPathSonos(
  base: string,
  token: string | undefined,
  opts?: RequestOptions,
): Promise<void> {
  await fetchStatus(base, opts);
  const inputs = await listInputs(base, token, opts);
  if (inputs[0]) await activateInput(base, inputs[0], token, opts);
  const outputs = await listOutputs(base, token, opts);
  const sonos = outputs.find((o) => o.transport === "sonos");
  if (!sonos) throw new Error("no sonos output");
  await activateOutput(base, "sonos", sonos.id, token, opts);
  await setVolume(base, 20, token, opts);
  await setEq(base, [3, 0, 0, 0, -3], token, opts);
}

/** Activate one device per transport in a fixed order; returns the order. */
export async function switchTransports(
  base: string,
  token?: string,
  opts?: RequestOptions,
): Promise<string[]> {
  const outputs = await listOutputs(base, token, opts);
  const order = ["sonos", "airplay", "bluetooth"] as const;
  const activated: string[] = [];
  for (const transport of order) {
    const device = outputs.find((o) => o.transport === transport);
    if (!device) throw new Error(`missing ${transport}`);
    await activateOutput(base, transport, device.id, token, opts);
    activated.push(transport);
  }
  return activated;
}

/**
 * Put a manual latency trim on the first output of `transport` and re-read
 * that output's row from `GET /api/sync`. The trim is a correction against
 * what the user actually hears — never a measurement.
 */
export async function trimFirstOutput(
  base: string,
  transport: Transport,
  trimMs: number,
  token?: string,
  opts?: RequestOptions,
): Promise<{ trimmed: { trim_ms: number; effective_latency_ms: number }; row: OutputSyncStatus }> {
  const outputs = await listOutputs(base, token, opts);
  const device = outputs.find((o) => o.transport === transport);
  if (!device) throw new Error(`missing ${transport}`);
  const trimmed = await setSyncTrim(
    base,
    { transport, device_id: device.id, trim_ms: trimMs },
    token,
    opts,
  );
  const sync = await getSync(base, token, opts);
  const row = sync.outputs.find((o) => o.transport === transport && o.device_id === device.id);
  if (!row) throw new Error(`${transport} ${device.id} missing from GET /api/sync`);
  return { trimmed, row };
}

/**
 * Create a named sync group out of the first `sonos` and `airplay` outputs
 * and start them together at ONE presentation epoch: several speakers
 * playing in sync, not one merged device. Returns the group as
 * `GET /api/sync` reports it after the start.
 */
export async function playSyncGroup(
  base: string,
  name: string,
  token?: string,
  opts?: RequestOptions,
): Promise<{ groupId: number; group: SyncGroupView }> {
  const outputs = await listOutputs(base, token, opts);
  const members = ["sonos", "airplay"].map((transport) => {
    const device = outputs.find((o) => o.transport === transport);
    if (!device) throw new Error(`missing ${transport}`);
    return { transport: transport as Transport, device_id: device.id };
  });
  const created = await createSyncGroup(base, name, token, opts);
  for (const member of members) {
    await addSyncGroupMember(base, created.id, member, token, opts);
  }
  await startSyncGroup(base, created.id, token, opts);
  const sync = await getSync(base, token, opts);
  const group = sync.groups?.find((candidate) => candidate.id === created.id);
  if (!group) throw new Error(`group ${created.id} missing from GET /api/sync`);
  return { groupId: created.id, group };
}
