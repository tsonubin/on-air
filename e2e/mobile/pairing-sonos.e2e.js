/* Detox E2E: iOS + Android pairing + Sonos control.
 * Drive the same mock core as packages/core tests (ON_AIR_MOCK=1).
 */
const { apiBase } = require("../../apps/mobile/src/controlClient.ts");

describe("pairing + Sonos control", () => {
  it("pairs with PIN and activates Sonos", async () => {
    if (typeof device !== "undefined") {
      await device.reloadReactNative();
      await element(by.id("host-input")).typeText("127.0.0.1");
      await element(by.id("pin-input")).typeText("123456");
      await element(by.id("pair-button")).tap();
      await expect(element(by.id("paired-token"))).toBeVisible();
      await element(by.id("output-sonos")).tap();
    } else {
      const { goldenPathSonos } = require("../../apps/mobile/src/controlClient.ts");
      const base = process.env.API_BASE || "http://127.0.0.1:47990";
      await goldenPathSonos(base, "123456");
    }
  });
});
