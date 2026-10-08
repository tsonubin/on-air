/**
 * Expo Go-compatible pairing regression loop.
 *
 * Uses only what Expo Go offers (HTTP fetch plus the shared TypeScript
 * control client). By default the desktop is handed to discovery as an extra
 * host, the way a typed or remembered address is, so the test does not depend
 * on the machine's network. Set E2E_LAN_SWEEP=1 on a host with a private IPv4
 * to also run the real /24 sweep against the mock core bound to 0.0.0.0.
 */
import assert from "node:assert/strict";
import { networkInterfaces } from "node:os";
import { after, before, test } from "node:test";
import {
  apiBase,
  discoverOnAir,
  type FetchLike,
  fetchStatus,
  listInputs,
  verifyPin,
} from "../../packages/control-client/src/index.ts";
import { type MockCore, startMockCore } from "../shared/mockCore.ts";

const PORT = Number(process.env.E2E_PORT ?? 47993);
const PIN = "123456";
const LAN_SWEEP = process.env.E2E_LAN_SWEEP === "1";
// A private address the phone could have on a LAN that does not exist here.
// If discovery swept it, the test would spend tens of seconds timing out.
const UNREACHABLE_PHONE_IP = "10.255.254.10";

let core: MockCore | undefined;

function privateLanIpv4(): string | undefined {
  for (const addresses of Object.values(networkInterfaces())) {
    for (const address of addresses ?? []) {
      if (address.family !== "IPv4" || address.internal) continue;
      if (
        address.address.startsWith("10.") ||
        address.address.startsWith("192.168.") ||
        /^172\.(1[6-9]|2\d|3[01])\./.test(address.address)
      ) {
        return address.address;
      }
    }
  }
  return undefined;
}

async function pairAndAuthenticate(host: string): Promise<void> {
  const base = apiBase(host, PORT);
  const status = await fetchStatus(base);
  assert.equal(status.status, "ok");
  const token = await verifyPin(base, PIN);
  assert.match(token, /^onair-/);
  const inputs = await listInputs(base, token);
  assert.ok(inputs.includes("Mock Monitor"));
}

before(async () => {
  core = await startMockCore({ port: PORT, bind: LAN_SWEEP ? "0.0.0.0" : "127.0.0.1" });
});

after(async () => {
  await core?.stop();
});

test("Expo Go finds a known desktop without sweeping, then PIN-pairs and authenticates", async () => {
  const probed: string[] = [];
  const countingFetch: FetchLike = (input, init) => {
    probed.push(new URL(String(input)).hostname);
    return fetch(input, init);
  };

  const discovered = await discoverOnAir({
    localIp: UNREACHABLE_PHONE_IP,
    port: PORT,
    extraHosts: ["127.0.0.1"],
    fetchImpl: countingFetch,
  });

  assert.deepEqual(
    discovered.map((hit) => `${hit.host}:${hit.port}`),
    [`127.0.0.1:${PORT}`],
  );
  assert.deepEqual(probed, ["127.0.0.1"], "a responsive extra host must skip the /24 sweep");
  await pairAndAuthenticate("127.0.0.1");
});

test("Expo Go sweeps the phone's /24 and pairs over the LAN", {
  skip: LAN_SWEEP ? false : "set E2E_LAN_SWEEP=1 on a host with a private IPv4",
}, async () => {
  const host = privateLanIpv4();
  assert.ok(host, "E2E_LAN_SWEEP=1 needs a private LAN IPv4 on this host");
  const discovered = await discoverOnAir({ localIp: host, port: PORT });
  assert.ok(discovered.some((candidate) => candidate.host === host));
  await pairAndAuthenticate(host);
});
