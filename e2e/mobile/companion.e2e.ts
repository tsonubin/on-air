/**
 * Headless Expo companion E2E against mock core.
 *
 * Covers the phone contract: discover the LAN control plane, PIN-pair, then
 * drive every mixer control the desktop UI exposes (source, destination,
 * exclusive transports, volume, EQ, sample rates, AirPlay/Bluetooth pairing).
 *
 * Simulators are not required. Detox UI lives in pairing-sonos.e2e.js.
 */
import assert from "node:assert/strict";
import { type ChildProcess, spawn } from "node:child_process";
import path from "node:path";
import { after, before, describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  activateInput,
  connectBluetooth,
  fetchStatus,
  getActiveInput,
  getActiveOutput,
  getAirplayMode,
  getEq,
  getSampleRate,
  goldenPathSonos,
  listBluetooth,
  listInputs,
  listOutputs,
  pairAirplay,
  pairBluetooth,
  probeOnAir,
  setSampleRate,
  switchTransports,
  verifyPin,
  wsUrl,
} from "../../packages/control-client/src/index.ts";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PORT = Number(process.env.E2E_PORT ?? 47992);
const BASE = `http://127.0.0.1:${PORT}`;
const PIN = "123456";

let child: ChildProcess | undefined;
let token = "";

function startMockCore(): Promise<ChildProcess> {
  return new Promise((resolve, reject) => {
    // Let Cargo validate the cached executable against the current source.
    // Executing target/debug/examples/serve directly can silently test a stale
    // binary restored by a CI cache.
    const spawned = spawn("cargo", ["run", "--locked", "-p", "on-air-core", "--example", "serve"], {
      cwd: ROOT,
      env: { ...process.env, ON_AIR_MOCK: "1", PORT: String(PORT), BIND: "127.0.0.1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const chunks: Buffer[] = [];
    let started = false;
    const timer = setTimeout(() => {
      spawned.kill();
      reject(new Error(`mock core did not start:\n${Buffer.concat(chunks).toString()}`));
    }, 120_000);
    const onData = (buf: Buffer) => {
      chunks.push(buf);
      if (!started && buf.toString().includes("listening")) {
        started = true;
        clearTimeout(timer);
        resolve(spawned);
      }
    };
    spawned.stdout?.on("data", onData);
    spawned.stderr?.on("data", onData);
    spawned.on("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });
    spawned.on("exit", (code) => {
      if (!started) {
        clearTimeout(timer);
        reject(
          new Error(`serve exited ${code ?? "by signal"}: ${Buffer.concat(chunks).toString()}`),
        );
      }
    });
  });
}

async function waitForStatus(retries = 40): Promise<void> {
  for (let i = 0; i < retries; i += 1) {
    try {
      await fetchStatus(BASE);
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  throw new Error(`mock core never answered ${BASE}/api/status`);
}

describe("expo companion against mock core", { concurrency: 1 }, () => {
  before(async () => {
    child = await startMockCore();
    await waitForStatus();
  });

  after(async () => {
    const running = child;
    if (!running || running.exitCode !== null) return;
    await new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve();
      };
      const timer = setTimeout(() => {
        running.kill("SIGKILL");
        finish();
      }, 5_000);
      running.once("exit", finish);
      running.kill("SIGTERM");
    });
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
    await goldenPathSonos(BASE, PIN);
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
    const transports = outputs.map((o) => o.transport).sort();
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
    const mode = await getAirplayMode(BASE, token);
    assert.ok(mode.length > 0);
    await activateInput(BASE, "Mock Monitor", token);
  });
});
