import { spawn, type ChildProcess } from "child_process";
import path from "path";
import {
  activateOutput,
  apiBase,
  fetchStatus,
  goldenPathSonos,
  listOutputs,
  switchTransports,
  verifyPin,
} from "../src/controlClient";

const ROOT = path.resolve(__dirname, "../../..");
const PORT = 47991;
const BASE = apiBase("127.0.0.1", PORT);

function startMockCore(): Promise<ChildProcess> {
  return new Promise((resolve, reject) => {
    const child = spawn(
      "cargo",
      ["run", "-p", "on-air-core", "--example", "serve", "--offline"],
      {
        cwd: ROOT,
        env: { ...process.env, ON_AIR_MOCK: "1", PORT: String(PORT) },
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    const timer = setTimeout(() => resolve(child), 8000);
    child.stderr?.on("data", (buf: Buffer) => {
      if (buf.toString().includes("listening")) {
        clearTimeout(timer);
        resolve(child);
      }
    });
    child.on("error", reject);
  });
}

async function waitForStatus(retries = 40): Promise<void> {
  for (let i = 0; i < retries; i++) {
    try {
      await fetchStatus(BASE);
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  throw new Error("mock core did not start");
}

describe("mobile control client against mock core", () => {
  let child: ChildProcess | undefined;

  beforeAll(async () => {
    child = await startMockCore();
    await waitForStatus();
  }, 60000);

  afterAll(() => {
    child?.kill();
  });

  it("pairs with the mock PIN and drives Sonos golden path", async () => {
    const status = await fetchStatus(BASE);
    expect(status.status).toBe("ok");
    const token = await verifyPin(BASE, "123456");
    expect(token.startsWith("onair-")).toBe(true);
    await goldenPathSonos(BASE, "123456");
    const outputs = await listOutputs(BASE);
    expect(outputs.some((o) => o.transport === "sonos")).toBe(true);
  }, 30000);

  it("switches Sonos to AirPlay to Bluetooth exclusively", async () => {
    const activated = await switchTransports(BASE);
    expect(activated).toEqual(["sonos", "airplay", "bluetooth"]);
    await activateOutput(BASE, "sonos", "uuid:mock-sonos");
  }, 30000);
});
