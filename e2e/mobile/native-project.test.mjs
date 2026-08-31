import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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
const androidApplication = read(
  "apps/mobile/android/app/src/main/java/app/onair/remote/MainApplication.kt",
);
const androidActivity = read(
  "apps/mobile/android/app/src/main/java/app/onair/remote/MainActivity.kt",
);
const iosPodfile = read("apps/mobile/ios/Podfile");
const iosAppDelegate = read("apps/mobile/ios/OnAirMobile/AppDelegate.swift");
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
  assert.match(iosProject, /IPHONEOS_DEPLOYMENT_TARGET = 16\.4;/);
});
