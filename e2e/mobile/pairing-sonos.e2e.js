/* Detox UI: iOS + Android pairing + mixer control against mock core (ON_AIR_MOCK=1).
 * Headless (no simulator) coverage lives in companion.e2e.ts.
 */
const detox = typeof device !== "undefined";

describe("pairing + Sonos control", () => {
  (detox ? it : it.skip)("pairs with PIN and drives the mixer from the Expo UI", async () => {
    await device.reloadReactNative();
    await element(by.id("manual-setup-button")).tap();
    await element(by.id("host-input")).replaceText("127.0.0.1");
    await element(by.id("pin-input")).typeText("123456");
    await element(by.id("pair-button")).tap();
    await expect(element(by.id("mixer-screen"))).toBeVisible();
    await element(by.id("source-row")).tap();
    await element(by.id("source-picker")).tap();
    await element(by.text("Mock Monitor")).tap();
    await element(by.id("output-row")).tap();
    await element(by.id("output-sonos-uuid:mock-sonos")).tap();
    await expect(element(by.text("Mock Sonos"))).toBeVisible();
    await element(by.id("volume-slider-native")).adjustSliderToPosition(0.51);
    await element(by.id("sound-settings-button")).tap();
    await element(by.text("Equalizer")).tap();
    await element(by.id("eq-band-0-native")).adjustSliderToPosition(0.63);
  });
});
