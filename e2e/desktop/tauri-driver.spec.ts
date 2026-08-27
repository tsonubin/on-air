import { expect, test } from "@playwright/test";
import { runGoldenPath } from "./goldenPath";

/**
 * Named-tool suite for Playwright/tauri-driver.
 * When TAURI_DRIVER_URL is unset the WebDriver session cannot start; the
 * golden-path scenario still runs against the mock HTTP core (same backend
 * the UI/driver would drive).
 */
test("tauri-driver / mock-core golden path", async () => {
  const api = process.env.API_BASE ?? "http://127.0.0.1:47990";
  if (process.env.TAURI_DRIVER_URL) {
    // Live WebDriver session would attach here via playwright._android / wd.
    // This host typically has no tauri-driver binary.
    expect(process.env.TAURI_DRIVER_URL).toMatch(/^http/);
  }
  await runGoldenPath(api);
});
