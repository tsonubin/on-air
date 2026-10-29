import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { flatPixels, layers as iconLayers } from "../../apps/mobile/scripts/make-app-icon.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (relativePath) => readFileSync(path.join(ROOT, relativePath), "utf8");

const appConfig = JSON.parse(read("apps/mobile/app.json")).expo;
const mobilePackage = JSON.parse(read("apps/mobile/package.json"));
const androidApp = read("apps/mobile/android/app/build.gradle");
const androidSettings = read("apps/mobile/android/settings.gradle");
const androidRoot = read("apps/mobile/android/build.gradle");
const androidManifest = read("apps/mobile/android/app/src/main/AndroidManifest.xml");
const androidStyles = read("apps/mobile/android/app/src/main/res/values/styles.xml");
const androidApplication = read(
  "apps/mobile/android/app/src/main/java/app/onair/remote/MainApplication.kt",
);
const androidActivity = read(
  "apps/mobile/android/app/src/main/java/app/onair/remote/MainActivity.kt",
);
const iosPodfile = read("apps/mobile/ios/Podfile");
const iosAppDelegate = read("apps/mobile/ios/OnAirMobile/AppDelegate.swift");
const iosSceneDelegate = read("apps/mobile/ios/OnAirMobile/SceneDelegate.swift");
const iosInfo = read("apps/mobile/ios/OnAirMobile/Info.plist");
const iosProject = read("apps/mobile/ios/OnAirMobile.xcodeproj/project.pbxproj");

test("native identifiers and versions stay aligned with Expo config", () => {
  const { version } = appConfig;
  const identifier = appConfig.ios.bundleIdentifier;

  assert.equal(mobilePackage.version, version);
  assert.equal(appConfig.android.package, identifier);
  assert.match(androidApp, new RegExp(`namespace ["']${identifier}["']`));
  assert.match(androidApp, new RegExp(`applicationId ["']${identifier}["']`));
  assert.match(androidApp, new RegExp(`versionName ["']${version.replaceAll(".", "\\.")}["']`));
  assert.ok(Number.isInteger(appConfig.android.versionCode));
  assert.match(androidApp, new RegExp(`versionCode ${appConfig.android.versionCode}\\n`));
  assert.match(iosProject, new RegExp(`MARKETING_VERSION = ${version.replaceAll(".", "\\.")};`));
  assert.match(
    iosProject,
    new RegExp(`PRODUCT_BUNDLE_IDENTIFIER = ${identifier.replaceAll(".", "\\.")};`),
  );
});

test("Android native project autolinks Expo modules and keeps release unsigned", () => {
  assert.match(androidSettings, /expo-autolinking-settings/);
  assert.match(androidSettings, /expoAutolinking\.useExpoModules\(\)/);
  assert.match(androidRoot, /apply plugin: ["']expo-root-project["']/);
  assert.match(androidApp, /bundleCommand = ["']export:embed["']/);
  assert.doesNotMatch(
    androidApp,
    /release\s*\{[^}]*signingConfig\s+signingConfigs\.debug/s,
    "release artifacts must never use the public debug key",
  );
  assert.match(androidApplication, /ExpoReactHostFactory/);
  assert.match(androidApplication, /ApplicationLifecycleDispatcher\.onApplicationCreate/);
  assert.match(androidActivity, /ReactActivityDelegateWrapper/);
  assert.match(androidManifest, /android\.permission\.INTERNET/);
  assert.match(androidManifest, /android:usesCleartextTraffic=["']true["']/);
});

test("iOS native project autolinks Expo and declares LAN discovery access", () => {
  assert.match(iosPodfile, /use_expo_modules!/);
  assert.match(iosPodfile, /expo-modules-autolinking/);
  assert.match(iosAppDelegate, /class AppDelegate: ExpoAppDelegate/);
  assert.match(iosAppDelegate, /ExpoReactNativeFactory/);
  assert.match(iosInfo, /<key>NSLocalNetworkUsageDescription<\/key>/);
  assert.match(iosInfo, /<key>NSBonjourServices<\/key>/);
  assert.match(iosInfo, /<string>_on-air\._tcp\.<\/string>/);
  assert.match(iosInfo, /<key>ITSAppUsesNonExemptEncryption<\/key>/);
  assert.match(iosProject, /IPHONEOS_DEPLOYMENT_TARGET = 16\.4;/);
  assert.match(iosProject, /lastKnownFileType = folder.iconcomposer/);
  assert.match(iosProject, /AppIcon.icon in Resources/);
  assert.match(iosProject, /ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;/);
  const nativeBuilds = [...iosProject.matchAll(/CURRENT_PROJECT_VERSION = (\d+);/g)];
  assert.ok(nativeBuilds.length > 0);
  for (const build of nativeBuilds) assert.equal(build[1], appConfig.ios.buildNumber);
  assert.equal(appConfig.ios.icon, "./assets/AppIcon.icon");
  assert.equal(appConfig.icon, "./assets/icon.png");
  assert.match(appConfig.ios.buildNumber, /^\d+$/);
  assert.equal(appConfig.ios.infoPlist.ITSAppUsesNonExemptEncryption, false);
  for (const relativePath of [
    "apps/mobile/assets/icon.png",
    "apps/mobile/assets/AppIcon.icon/icon.json",
    "apps/mobile/ios/OnAirMobile/AppIcon.icon/icon.json",
    "apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/AppIcon.png",
  ]) {
    assert.ok(existsSync(path.join(ROOT, relativePath)), relativePath);
  }
  const marketingCatalog = JSON.parse(
    read("apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/Contents.json"),
  );
  assert.ok(
    marketingCatalog.images.some(
      (image) => image.idiom === "universal" && image.filename === "AppIcon.png",
    ),
  );
});

test("iOS adopts the scene life cycle the iOS 27 SDK requires", () => {
  // The iOS 27 SDK asserts at launch unless the app adopts UIScene. Expo's scene
  // delegate creates the window and starts React Native from the app delegate's factory.
  assert.match(
    iosAppDelegate,
    /class AppDelegate: ExpoAppDelegate, ExpoReactNativeFactoryProvider/,
  );
  assert.doesNotMatch(iosAppDelegate, /startReactNative/);
  assert.match(iosSceneDelegate, /class SceneDelegate: ExpoAppSceneDelegate/);
  assert.match(iosProject, /SceneDelegate\.swift in Sources/);
  assert.match(iosInfo, /<key>UIApplicationSceneManifest<\/key>/);
  assert.match(
    iosInfo,
    /<key>UISceneDelegateClassName<\/key>\s*<string>\$\(PRODUCT_MODULE_NAME\)\.SceneDelegate<\/string>/,
  );
});

test("iOS native project keeps the dark-only Expo appearance", () => {
  assert.equal(appConfig.userInterfaceStyle, "dark");
  assert.match(iosInfo, /<key>UIUserInterfaceStyle<\/key>\s*<string>Dark<\/string>/);
});

test("Android native project keeps the dark-only Expo appearance", () => {
  // userInterfaceStyle needs expo-system-ui on Android. Forcing AppCompat's night mode puts
  // UI_MODE_NIGHT_YES in the configuration, which the DayNight theme, React Native's
  // navigation bar style and Expo UI's unseeded Compose hosts all read.
  assert.equal(appConfig.userInterfaceStyle, "dark");
  assert.match(androidStyles, /name="AppTheme" parent="Theme\.AppCompat\.DayNight\./);
  // The system draws the launch splash from the theme before any app code runs, under the
  // system's own night mode, so the window background must be dark in both configurations.
  assert.equal(appConfig.backgroundColor, "#000000");
  assert.match(
    androidStyles,
    /<item name="android:windowBackground">@android:color\/black<\/item>/,
  );
  assert.match(
    androidApplication,
    /AppCompatDelegate\.setDefaultNightMode\(AppCompatDelegate\.MODE_NIGHT_YES\)/,
  );
});

const REGENERATE = "stale: run `node scripts/make-app-icon.mjs` from apps/mobile";

// Icon Composer stores a value as `key` until it varies by appearance, then as
// `key-specializations`, whose entry without an appearance is the default.
const iconValue = (object, key, appearance) =>
  appearance === undefined && key in object
    ? object[key]
    : object[`${key}-specializations`]?.find((entry) => entry.appearance === appearance)?.value;

test("Icon Composer packages hold the layered Liquid Glass Live Dial", () => {
  const packages = ["apps/mobile/assets/AppIcon.icon", "apps/mobile/ios/OnAirMobile/AppIcon.icon"];
  const specs = packages.map((folder) => JSON.parse(read(`${folder}/icon.json`)));
  assert.deepEqual(specs[0], specs[1], `the Xcode copy is ${REGENERATE}`);
  const [spec] = specs;
  // The charcoal background is the document fill, with its own dark appearance.
  assert.ok(iconValue(spec, "fill"));
  assert.ok(iconValue(spec, "fill", "dark"));
  const layers = spec.groups.flatMap((group) => group.layers);
  assert.deepEqual(
    layers.map((layer) => layer["image-name"]).sort(),
    Object.keys(iconLayers).sort(),
  );
  for (const group of spec.groups) {
    assert.equal(iconValue(group, "specular"), true);
    assert.equal(iconValue(group, "translucency")?.enabled, true);
    assert.ok(iconValue(group, "shadow")?.opacity > 0);
  }
  for (const layer of layers) {
    assert.equal(layer.glass, true);
    assert.ok(iconValue(layer, "fill", "tinted"), `${layer.name} needs a tinted fill`);
  }
  for (const folder of packages) {
    const assets = path.join(ROOT, folder, "Assets");
    const files = readdirSync(assets).filter((name) => !name.startsWith("."));
    assert.deepEqual(files.sort(), Object.keys(iconLayers).sort());
    for (const [name, svg] of Object.entries(iconLayers)) {
      assert.equal(
        readFileSync(path.join(assets, name), "utf8"),
        svg,
        `${folder} is ${REGENERATE}`,
      );
    }
  }
});

test("flat icon fallbacks are opaque 1024 px renders of the layers", () => {
  const expected = flatPixels();
  for (const relativePath of [
    "apps/mobile/assets/icon.png",
    "apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/AppIcon.png",
  ]) {
    const png = readFileSync(path.join(ROOT, relativePath));
    // App Store icons must be opaque: 8-bit RGB, no tRNS transparency chunk.
    assert.equal(png.toString("ascii", 12, 16), "IHDR");
    assert.equal(png.readUInt32BE(16), 1024);
    assert.equal(png.readUInt32BE(20), 1024);
    assert.equal(png[24], 8, "bit depth");
    assert.equal(png[25], 2, "color type: RGB");
    const idat = [];
    for (let at = 8; at < png.length; at += png.readUInt32BE(at) + 12) {
      const type = png.toString("ascii", at + 4, at + 8);
      assert.notEqual(type, "tRNS");
      if (type === "IDAT") idat.push(png.subarray(at + 8, at + 8 + png.readUInt32BE(at)));
    }
    // Compare pixels, not bytes: zlib output varies across Node builds. A sample
    // flipped by floating-point noise moves a channel by at most 7.
    const actual = inflateSync(Buffer.concat(idat));
    assert.equal(actual.length, expected.length);
    let worst = 0;
    for (let i = 0; i < actual.length; i += 1) {
      worst = Math.max(worst, Math.abs(actual[i] - expected[i]));
    }
    assert.ok(worst <= 8, `${relativePath} is ${REGENERATE}`);
  }
});
