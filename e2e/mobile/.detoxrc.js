/** @type {Detox.DetoxConfig} */
module.exports = {
  testRunner: {
    args: { $0: "jest", config: "e2e/mobile/jest.config.js" },
    jest: { setupTimeout: 120000 },
  },
  apps: {
    "ios.debug": {
      type: "ios.app",
      binaryPath: "apps/mobile/ios/build/Build/Products/Debug-iphonesimulator/OnAirMobile.app",
      build:
        "xcodebuild -workspace apps/mobile/ios/OnAirMobile.xcworkspace -scheme OnAirMobile -configuration Debug -sdk iphonesimulator -derivedDataPath apps/mobile/ios/build",
    },
    "android.debug": {
      type: "android.apk",
      binaryPath: "apps/mobile/android/app/build/outputs/apk/debug/app-debug.apk",
      build:
        "cd apps/mobile/android && ./gradlew assembleDebug assembleAndroidTest -DtestBuildType=debug",
    },
  },
  devices: {
    simulator: { type: "ios.simulator", device: { type: "iPhone 15" } },
    emulator: { type: "android.emulator", device: { avdName: "Pixel_5_API_34" } },
  },
  configurations: {
    "ios.sim.debug": { device: "simulator", app: "ios.debug" },
    "android.emu.debug": { device: "emulator", app: "android.debug" },
  },
};
