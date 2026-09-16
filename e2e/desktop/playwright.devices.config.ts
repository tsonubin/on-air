import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: ".",
  testMatch: "device-setup.spec.ts",
  workers: 1,
  use: { baseURL: "http://127.0.0.1:1422" },
  webServer: {
    command: "pnpm --filter desktop dev --port 1422",
    url: "http://127.0.0.1:1422",
    reuseExistingServer: true,
    cwd: "../..",
  },
});
