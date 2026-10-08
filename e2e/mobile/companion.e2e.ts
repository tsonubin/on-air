/**
 * Headless Expo companion E2E against the mock core.
 *
 * Covers the phone contract: reach the LAN control plane, PIN-pair, then
 * drive every mixer control the desktop UI exposes (source, destination,
 * exclusive transports, volume, EQ, sample rates, AirPlay/Bluetooth pairing).
 * No simulator is needed.
 */
import assert from "node:assert/strict";
import { after, before, describe, test } from "node:test";
import {
  activateInput,
  connectBluetooth,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getEq,
  getSampleRate,
  listBluetooth,
  listInputs,
  listOutputs,
  openBluetoothSettings,
  pairAirplay,
  pairBluetooth,
  probeOnAir,
  setSampleRate,
  verifyPin,
  wsUrl,
} from "../../packages/control-client/src/index.ts";
import { type MockCore, startMockCore } from "../shared/mockCore.ts";
import { goldenPathSonos, switchTransports } from "../shared/scenarios.ts";

const PORT = Number(process.env.E2E_PORT ?? 47992);
const BASE = `http://127.0.0.1:${PORT}`;
const PIN = "123456";

let core: MockCore | undefined;
let token = "";

describe("expo companion against mock core", { concurrency: 1 }, () => {
  before(async () => {
    core = await startMockCore({ port: PORT });
  });

  after(async () => {
    await core?.stop();
  });

  test("phone discovers the LAN control plane", async () => {
    const hit = await probeOnAir("127.0.0.1", PORT, 1000);
    assert.ok(hit, "expected /api/status from mock core");
    assert.equal(hit.host, "127.0.0.1");
    assert.equal(hit.port, PORT);
    assert.equal(hit.name, "on-air");
    const status = await fetchStatus(BASE);
    assert.equal(status.status, "ok");
  });

  test("PIN pair issues a token the rest of the mixer can use", async () => {
    token = await verifyPin(BASE, PIN);
    assert.match(token, /^onair-/);
    assert.equal(wsUrl(BASE, token), `ws://127.0.0.1:${PORT}/api/ws?token=${token}`);
  });

  test("companion golden path: source, Sonos, volume, EQ", async () => {
    await goldenPathSonos(BASE, token);
    const eq = await getEq(BASE, token);
    assert.deepEqual(eq, [3, 0, 0, 0, -3]);
    const active = await getActiveOutput(BASE, token);
    assert.equal(active?.transport, "sonos");
    const inputs = await listInputs(BASE, token);
    assert.ok(inputs.includes("Mock Monitor"));
    const chosen = await getActiveInput(BASE, token);
    assert.equal(chosen.name, "Mock Monitor");
  });

  test("companion lists every exclusive transport and can switch them", async () => {
    const outputs = await listOutputs(BASE, token);
    const transports = [...new Set(outputs.map((o) => o.transport))].sort();
    assert.deepEqual(transports, ["airplay", "bluetooth", "sonos"]);
    const switched = await switchTransports(BASE, token);
    assert.deepEqual(switched, ["sonos", "airplay", "bluetooth"]);
    const active = await getActiveOutput(BASE, token);
    assert.equal(active?.transport, "bluetooth");
  });

  test("companion can set input and output sample rates", async () => {
    await setSampleRate(BASE, { input_hz: 48000, output_hz: 44100 }, token);
    const rates = await getSampleRate(BASE, token);
    assert.equal(rates.input.sample_rate_hz, 48000);
    assert.equal(rates.output.sample_rate_hz, 44100);
    assert.ok(rates.input.supported_hz.includes(44100));
    assert.ok(rates.output.supported_hz.includes(44100));
  });

  test("companion can pair AirPlay and Bluetooth destinations", async () => {
    await pairAirplay(BASE, "ap-living", "1111", token);
    await pairBluetooth(BASE, "bt-speaker", token);
    await connectBluetooth(BASE, "bt-speaker", token);
    const devices = await listBluetooth(BASE, token);
    const speaker = devices.find((d) => d.id === "bt-speaker");
    assert.ok(speaker, "expected mock bluetooth speaker");
    assert.equal(speaker.paired, true);
    await openBluetoothSettings(BASE, token);
    const mode = await getAirplayMode(BASE, token);
    assert.ok(mode.length > 0);
    await activateInput(BASE, "Mock Monitor", token);
  });
});
