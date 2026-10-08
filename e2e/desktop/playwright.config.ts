import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "@playwright/test";

// Projects:
//   api  HTTP golden paths against the mock core; no browser.
//   ui   the desktop frontend in Chromium, served by Vite, talking to the
//        same mock core (device-setup stubs every /api route instead).
// Run one with `--project api` or `--project ui`. Both web servers start
// either way; Vite is quick and the core is shared.
//
// The desktop frontend calls http://127.0.0.1:47990, so the mock core must
// own that port. It is never reused: a running real core there would turn
// these tests into commands for real speakers.
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const CORE_PORT = 47990;
const UI_URL = "http://127.0.0.1:1420";

export default defineConfig({
  testDir: ".",
  timeout: 60_000,
  retries: 0,
  // Both projects drive one mock core; run serially so they do not race.
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { trace: "retain-on-failure" },
  projects: [
    {
      name: "api",
      testMatch: "golden-path.spec.ts",
      grep: /@api/,
    },
    {
      name: "ui",
      testMatch: ["device-setup.spec.ts", "golden-path.spec.ts"],
      grepInvert: /@api/,
      use: { browserName: "chromium", baseURL: UI_URL },
    },
  ],
  webServer: [
    {
      command: "cargo run --locked -p on-air-core --bin on-air-core",
      url: `http://127.0.0.1:${CORE_PORT}/api/status`,
      reuseExistingServer: false,
      // First run compiles the core.
      timeout: 300_000,
      stdout: "pipe",
      stderr: "pipe",
      cwd: ROOT,
      env: { ON_AIR_MOCK: "1", PORT: String(CORE_PORT), BIND: "127.0.0.1" },
    },
    {
      command: "pnpm --filter desktop dev",
      url: UI_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      cwd: ROOT,
    },
  ],
});
