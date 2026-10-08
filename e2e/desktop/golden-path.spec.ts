import { expect, test } from "@playwright/test";
import { getActiveOutput, getEq, simulateCd } from "../../packages/control-client/src/index.ts";
import { goldenPathSonos, switchTransports } from "../shared/scenarios.ts";

// The mock core started by playwright.config.ts. Loopback callers need no
// pairing token.
const API = "http://127.0.0.1:47990";

test("API golden path: pick input, activate Sonos, volume, EQ", { tag: "@api" }, async () => {
  await goldenPathSonos(API, undefined);
  expect((await getEq(API))[0]).toBe(3);
  expect((await getActiveOutput(API))?.transport).toBe("sonos");
});

test("API transport switch Sonos -> AirPlay -> Bluetooth", { tag: "@api" }, async () => {
  const activated = await switchTransports(API);
  expect(activated).toEqual(["sonos", "airplay", "bluetooth"]);
  expect((await getActiveOutput(API))?.transport).toBe("bluetooth");
});

test("desktop UI plays an inserted audio CD", async ({ page }) => {
  await simulateCd(API, {
    present: true,
    album: "Kind of Blue",
    tracks: [{ title: "So What" }, { title: "Freddie Freeloader" }],
  });
  try {
    await page.goto("/");
    await expect(page.getByTestId("cd-transport")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("cd-track")).toContainText("01/02");
    await expect(page.getByTestId("cd-transport")).toContainText("So What");
    await page.getByTestId("cd-next").click();
    await expect(page.getByTestId("cd-track")).toContainText("02/02");
    await page.getByTestId("cd-play").click();
    await expect(page.getByTestId("cd-play")).toHaveAttribute("aria-label", "Play");
  } finally {
    await simulateCd(API, { present: false });
  }
});

test("desktop UI golden path against mock core", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("core-status")).toContainText("ok", { timeout: 15_000 });
  await page.getByTestId("input-Mock Monitor").click();
  await page.getByTestId("output-sonos-uuid:mock-sonos").click();
  await expect(page.getByTestId("active-output")).toContainText("sonos");
  await page.getByTestId("volume-slider").fill("20");
  await page.getByTestId("eq-band-0").fill("3");
});
