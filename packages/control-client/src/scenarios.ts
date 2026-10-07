// Deprecated test scenarios kept so existing imports keep compiling during the
// quality overhaul. The live copies are in e2e/shared/scenarios.ts; Phase 3
// (WP-F) deletes this file together with the re-exports in index.ts.
import {
  activateInput,
  activateOutput,
  type FetchLike,
  fetchStatus,
  listInputs,
  listOutputs,
  setEq,
  setVolume,
  verifyPin,
} from "./http.ts";

/** @deprecated Moved to `e2e/shared/scenarios.ts`; removed in Phase 3. */
export async function goldenPathSonos(
  base: string,
  pin: string,
  fetchImpl?: FetchLike,
): Promise<void> {
  const opts = { fetchImpl };
  await fetchStatus(base, opts);
  const token = await verifyPin(base, pin, opts);
  const inputs = await listInputs(base, token, opts);
  if (inputs[0]) await activateInput(base, inputs[0], token, opts);
  const outputs = await listOutputs(base, token, opts);
  const sonos = outputs.find((o) => o.transport === "sonos");
  if (!sonos) throw new Error("no sonos output");
  await activateOutput(base, "sonos", sonos.id, token, opts);
  await setVolume(base, 20, token, opts);
  await setEq(base, [3, 0, 0, 0, -3], token, opts);
}

/** @deprecated Moved to `e2e/shared/scenarios.ts`; removed in Phase 3. */
export async function switchTransports(
  base: string,
  token?: string,
  fetchImpl?: FetchLike,
): Promise<string[]> {
  const opts = { fetchImpl };
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
