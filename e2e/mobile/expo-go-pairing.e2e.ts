/**
 * Expo Go-compatible LAN pairing regression loop.
 *
 * This intentionally reaches the mock desktop through a private LAN address,
 * not loopback, and uses only APIs available in Expo Go (HTTP fetch plus the
 * shared TypeScript control client).
 */
import assert from "node:assert/strict";
import { type ChildProcess, spawn } from "node:child_process";
import { networkInterfaces } from "node:os";
import path from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  apiBase,
  discoverOnAir,
  fetchStatus,
  listInputs,
  verifyPin,
} from "../../packages/control-client/src/index.ts";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PORT = Number(process.env.E2E_PORT ?? 47993);
const PIN = "123456";

let child: ChildProcess | undefined;

function privateLanIpv4(): string {
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
  throw new Error("no private LAN IPv4 available for Expo Go pairing test");
}

function startMockCore(): Promise<ChildProcess> {
  return new Promise((resolve, reject) => {
    const spawned = spawn("cargo", ["run", "--locked", "-p", "on-air-core", "--example", "serve"], {
      cwd: ROOT,
      env: { ...process.env, ON_AIR_MOCK: "1", PORT: String(PORT), BIND: "0.0.0.0" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const chunks: Buffer[] = [];
    let started = false;
    const timer = setTimeout(() => {
      spawned.kill();
      reject(new Error(`mock core did not start:\n${Buffer.concat(chunks).toString()}`));
    }, 120_000);
    const onData = (buffer: Buffer) => {
      chunks.push(buffer);
      if (!started && buffer.toString().includes("listening")) {
        started = true;
        clearTimeout(timer);
        resolve(spawned);
      }
    };
    spawned.stdout?.on("data", onData);
    spawned.stderr?.on("data", onData);
    spawned.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    spawned.on("exit", (code) => {
      if (started) return;
      clearTimeout(timer);
      reject(
        new Error(`mock core exited ${code ?? "by signal"}: ${Buffer.concat(chunks).toString()}`),
      );
    });
  });
}

before(async () => {
  child = await startMockCore();
});

after(async () => {
  const running = child;
  if (!running || running.exitCode !== null) return;
  await new Promise<void>((resolve) => {
    const timer = setTimeout(() => {
      running.kill("SIGKILL");
      resolve();
    }, 5_000);
    running.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
    running.kill("SIGTERM");
  });
});

test("Expo Go can discover, manually address, PIN-pair, and authenticate over the LAN", async () => {
  const host = privateLanIpv4();
  const base = apiBase(host, PORT);

  const discovered = await discoverOnAir({ localIp: host, port: PORT });
  assert.ok(discovered.some((candidate) => candidate.host === host));

  const status = await fetchStatus(base);
  assert.equal(status.status, "ok");

  const token = await verifyPin(base, PIN);
  assert.match(token, /^onair-/);

  const inputs = await listInputs(base, token);
  assert.ok(inputs.includes("Mock Monitor"));
});
