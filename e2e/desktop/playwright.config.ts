import { defineConfig } from "@playwright/test";

const api = process.env.API_BASE ?? "http://127.0.0.1:47990";

export default defineConfig({
  testDir: ".",
  timeout: 60_000,
  retries: 0,
  use: { baseURL: process.env.UI_BASE ?? "http://127.0.0.1:1420" },
  webServer: [
    {
      command: "cargo run -p on-air-core --example serve",
      url: `${api}/api/status`,
      reuseExistingServer: true,
      timeout: 120_000,
      stdout: "pipe",
      stderr: "pipe",
      cwd: "../..",
      env: { ...process.env, ON_AIR_MOCK: "1", PORT: "47990" },
    },
  ],
});
