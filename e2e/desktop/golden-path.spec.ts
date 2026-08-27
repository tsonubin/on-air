import { expect, test } from "@playwright/test";
import { runGoldenPath, runTransportSwitch } from "./goldenPath";

const API = process.env.API_BASE ?? "http://127.0.0.1:47990";

test("API golden path: pick input, activate Sonos, volume, EQ", async () => {
  await runGoldenPath(API);
  const eq = await fetch(`${API}/api/eq`).then((r) => r.json());
  expect(eq.gains_db[0]).toBe(3);
  const active = await fetch(`${API}/api/outputs/active`).then((r) => r.json());
  expect(active.transport).toBe("sonos");
});

test("API transport switch Sonos -> AirPlay -> Bluetooth", async () => {
  const activated = await runTransportSwitch(API);
  expect(activated).toEqual(["sonos", "airplay", "bluetooth"]);
});

test("desktop UI golden path against mock core", async ({ page }) => {
  const ui = process.env.UI_BASE ?? "http://127.0.0.1:1420";
  try {
    await page.goto(ui, { timeout: 5_000 });
  } catch {
    test.skip(true, "desktop vite UI is not running on :1420");
    return;
  }
  await expect(page.getByTestId("core-status")).toContainText("ok", { timeout: 15_000 });
  const input = page.getByTestId("input-Mock Monitor");
  await input.click();
  await page.getByTestId("output-sonos-uuid:mock-sonos").click();
  await expect(page.getByTestId("active-output")).toContainText("sonos");
  await page.getByTestId("volume-slider").fill("20");
  await page.getByTestId("eq-band-0").fill("3");
});
