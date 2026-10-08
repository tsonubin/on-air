// Starts the on-air-core binary in mock mode for the e2e suites and stops it
// again. `cargo run` (not the built executable) so Cargo checks the cached
// binary against the current source: a CI cache can otherwise restore a
// stale build.
import { type ChildProcess, spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Arguments for `cargo` that build and run the mock core. */
export const MOCK_CORE_CARGO_ARGS = [
  "run",
  "--locked",
  "-p",
  "on-air-core",
  "--bin",
  "on-air-core",
] as const;

export interface MockCoreOptions {
  port: number;
  /** Address to bind; defaults to loopback. */
  bind?: string;
  /** How long to wait for the first build and `/api/status`. Default 180 s. */
  startTimeoutMs?: number;
}

export interface MockCore {
  /** `http://127.0.0.1:<port>`; reachable whatever `bind` is. */
  base: string;
  port: number;
  stop(): Promise<void>;
}

const POLL_MS = 250;
const STOP_GRACE_MS = 5_000;

async function statusOk(base: string): Promise<boolean> {
  try {
    const response = await fetch(`${base}/api/status`, { signal: AbortSignal.timeout(1_000) });
    return response.ok;
  } catch {
    return false;
  }
}

function stopChild(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(() => child.kill("SIGKILL"), STOP_GRACE_MS);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
    child.kill("SIGTERM");
  });
}

/**
 * Spawn the mock core and resolve once `GET /api/status` answers. Rejects
 * with the captured output if the process exits or the timeout passes first.
 */
export async function startMockCore(opts: MockCoreOptions): Promise<MockCore> {
  const { port, bind = "127.0.0.1", startTimeoutMs = 180_000 } = opts;
  const base = `http://127.0.0.1:${port}`;
  if (await statusOk(base)) {
    throw new Error(`port ${port} already answers /api/status; stop that core first`);
  }

  const child = spawn("cargo", [...MOCK_CORE_CARGO_ARGS], {
    cwd: REPO_ROOT,
    env: { ...process.env, ON_AIR_MOCK: "1", PORT: String(port), BIND: bind },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output: Buffer[] = [];
  child.stdout?.on("data", (chunk: Buffer) => output.push(chunk));
  child.stderr?.on("data", (chunk: Buffer) => output.push(chunk));
  let spawnError: Error | undefined;
  child.on("error", (error) => {
    spawnError = error;
  });

  const deadline = Date.now() + startTimeoutMs;
  while (!(await statusOk(base))) {
    const exited = child.exitCode !== null || child.signalCode !== null;
    if (spawnError || exited || Date.now() > deadline) {
      await stopChild(child);
      const reason = spawnError
        ? `failed to spawn cargo: ${spawnError.message}`
        : exited
          ? `exited with ${child.exitCode ?? child.signalCode}`
          : `did not answer ${base}/api/status within ${startTimeoutMs} ms`;
      throw new Error(`mock core ${reason}\n${Buffer.concat(output).toString()}`);
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
  }

  return { base, port, stop: () => stopChild(child) };
}
