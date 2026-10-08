// Shared end-to-end scenarios run against a mock core by the desktop and
// mobile e2e suites. They live here, not in the production control client,
// so the client ships no test choreography. Import the client the way the
// other e2e files do: by relative path, so no build step is needed.
import {
  activateInput,
  activateOutput,
  fetchStatus,
  listInputs,
  listOutputs,
  type RequestOptions,
  setEq,
  setVolume,
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
