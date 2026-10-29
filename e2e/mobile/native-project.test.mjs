import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

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

test("Icon Composer packages hold the layered Liquid Glass Live Dial", () => {
  const packages = ["apps/mobile/assets/AppIcon.icon", "apps/mobile/ios/OnAirMobile/AppIcon.icon"];
  const specs = packages.map((folder) => JSON.parse(read(`${folder}/icon.json`)));
  assert.deepEqual(specs[0], specs[1]);
  const [spec] = specs;
  // The charcoal background is the document fill, with its own dark appearance.
  assert.ok(spec["fill-specializations"].some((fill) => !fill.appearance));
  assert.ok(spec["fill-specializations"].some((fill) => fill.appearance === "dark"));
  const layers = spec.groups.flatMap((group) => group.layers);
  assert.deepEqual(layers.map((layer) => layer["image-name"]).sort(), [
    "dial-glyph.svg",
    "on-air-dot.svg",
  ]);
  for (const group of spec.groups) {
    assert.equal(group.specular, true);
    assert.equal(group.translucency.enabled, true);
    assert.ok(group.shadow.opacity > 0);
  }
  for (const layer of layers) {
    assert.equal(layer.glass, true);
    assert.ok(layer["fill-specializations"].some((fill) => fill.appearance === "tinted"));
  }
  for (const folder of packages) {
    const assets = path.join(ROOT, folder, "Assets");
    assert.deepEqual(readdirSync(assets).sort(), ["dial-glyph.svg", "on-air-dot.svg"]);
    for (const layer of layers) {
      const svg = readFileSync(path.join(assets, layer["image-name"]), "utf8");
      assert.match(svg, /^<svg [^>]*viewBox="0 0 1024 1024"/);
      assert.equal(svg, read(`${packages[0]}/Assets/${layer["image-name"]}`));
    }
  }
});

test("flat icon fallbacks are the same opaque 1024 px render", () => {
  const icon = readFileSync(path.join(ROOT, "apps/mobile/assets/icon.png"));
  assert.deepEqual(
    readFileSync(
      path.join(ROOT, "apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/AppIcon.png"),
    ),
    icon,
  );
  assert.equal(icon.toString("ascii", 12, 16), "IHDR");
  assert.equal(icon.readUInt32BE(16), 1024);
  assert.equal(icon.readUInt32BE(20), 1024);
  assert.equal(icon[25], 2, "8-bit RGB without alpha");
});
