import type {
  ActiveInputResponse,
  ActiveOutputResponse,
  AirPlayModeResponse,
  CdStatus,
  EqResponse,
  InputsResponse,
  OutputsResponse,
  OutputVolumeResponse,
  PinResponse,
  SampleRateResponse,
  StatusResponse,
} from "@on-air/api-types";
import { expect, test } from "@playwright/test";

// Every /api route is stubbed, so these tests need no core. Each fixture is
// checked against the shared API types so it cannot drift from the core.
const responses: Record<string, unknown> = {
  "/api/status": { status: "ok", version: "0.1.0", service_enabled: true } satisfies StatusResponse,
  "/api/inputs": { inputs: ["Studio microphone", "System audio"] } satisfies InputsResponse,
  "/api/inputs/active": { name: null, backend: "cpal-default" } satisfies ActiveInputResponse,
  "/api/outputs": {
    outputs: [
      {
        id: "air",
        name: "Living room speaker",
        transport: "airplay",
        kind: "solo",
        member_count: 1,
        needs_pair: false,
        paired: true,
      },
      {
        id: "bt",
        name: "Studio headphones",
        transport: "bluetooth",
        kind: "solo",
        member_count: 1,
        needs_pair: false,
        paired: true,
      },
    ],
  } satisfies OutputsResponse,
  "/api/outputs/active": { active: null } satisfies ActiveOutputResponse,
  "/api/outputs/active/volume": { volume: 50 } satisfies OutputVolumeResponse,
  "/api/eq": { gains_db: [0, 0, 0, 0, 0] } satisfies EqResponse,
  "/api/sample-rate": {
    sample_rate_hz: 44100,
    input: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
    output: { sample_rate_hz: 44100, supported_hz: [44100, 48000] },
  } satisfies SampleRateResponse,
  "/api/pairing/pin": { pin: "123456" } satisfies PinResponse,
  "/api/airplay/mode": { mode: "avroute-picker" } satisfies AirPlayModeResponse,
  "/api/cd": {
    present: false,
    playing: false,
    track: 0,
    track_count: 0,
    position_ms: 0,
    duration_ms: 0,
    tracks: [],
  } satisfies CdStatus,
};

let requests: string[];
test.beforeEach(async ({ page }) => {
  requests = [];
  await page.routeWebSocket("**/api/ws*", (socket) => socket.close());
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    requests.push(`${request.method()} ${path}`);
    if (request.method() !== "GET") return route.fulfill({ status: 204 });
    await route.fulfill({ json: responses[path] ?? {} });
  });
  await page.goto("/");
  await expect(page.getByTestId("output-bluetooth-bt")).toBeVisible();
});
for (const width of [800, 1024, 1440]) {
  test(`device lists align at ${width}px even when actions wrap`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 800 });
    const input = await page.getByTestId("input-list").boundingBox();
    const output = await page.getByTestId("output-list").boundingBox();
    expect(input).not.toBeNull();
    expect(output).not.toBeNull();
    if (!input || !output) throw new Error("Device lists must be laid out");
    expect(Math.abs(input.y - output.y)).toBeLessThan(1);
    const sourceHeading = await page
      .getByRole("heading", { name: "Source", exact: true })
      .boundingBox();
    const destinationHeading = await page
      .getByRole("heading", { name: "Destination", exact: true })
      .boundingBox();
    if (!sourceHeading || !destinationHeading) throw new Error("Headings must be laid out");
    expect(Math.abs(sourceHeading.y - destinationHeading.y)).toBeLessThan(1);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    if (width === 1024) {
      await page.screenshot({ path: testInfo.outputPath("device-layout.png") });
    }
  });
}
test("Bluetooth handoff explains the next step without activating a speaker", async ({ page }) => {
  await page.getByTestId("add-bluetooth").click();
  await expect(page.getByRole("status")).toContainText("Opening settings does not start playback");
  await expect.poll(() => requests.includes("POST /api/bluetooth/settings")).toBe(true);
  expect(requests).not.toContain("POST /api/outputs/active");
  const count = requests.filter((r) => r === "GET /api/outputs").length;
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect
    .poll(() => requests.filter((r) => r === "GET /api/outputs").length)
    .toBeGreaterThan(count);
  await page.getByRole("button", { name: "Refresh devices", exact: true }).click();
  await expect(page.getByRole("button", { name: "Refresh devices", exact: true })).toBeEnabled();
});
test("unavailable AirPlay explains its limitation instead of sending activation", async ({
  page,
}) => {
  await page.getByTestId("output-airplay-air").click();
  await expect(page.getByRole("status")).toContainText("does not start this mixer");
  expect(requests).not.toContain("POST /api/outputs/active");
  await page.setViewportSize({ width: 600, height: 800 });
  await expect(page.getByRole("button", { name: "Dismiss", exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
});
test("a pending connection blocks competing selections", async ({ page }) => {
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  await page.route("**/api/outputs/active", async (route) => {
    if (route.request().method() === "GET") return route.fulfill({ json: { active: null } });
    await pending;
    await route.fulfill({ status: 204 });
  });
  await page.getByTestId("output-bluetooth-bt").click();
  await expect(page.getByTestId("output-bluetooth-bt")).toContainText("Connecting…");
  await expect(page.getByTestId("output-airplay-air")).toBeDisabled();
  finish();
  await expect(page.getByTestId("output-bluetooth-bt")).toBeEnabled();
});
