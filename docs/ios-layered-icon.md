# iOS layered / Icon Composer home-screen icon

## Status

Blocked on design assets. Wiring is documented below; do not invent artwork.

## Root cause (verified)

The iOS 26/27 multi-layer / Icon Composer / liquid-glass home-screen depth
effect is missing because **this repo has never shipped an Icon Composer
`.icon` bundle, and the legacy App Icon catalog has no images**.

Evidence:

| Check | Result |
| --- | --- |
| `**/*.icon` / `**/icon.json` | none in the tree or git history |
| `apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/` | `Contents.json` only; every slot lacks a `filename`; no PNG/PDF/SVG |
| `apps/mobile/app.json` | no top-level `icon`, no `ios.icon` |
| Xcode `ASSETCATALOG_COMPILER_APPICON_NAME` | `AppIcon` in Debug and Release |
| `Info.plist` | no `CFBundleIcons` / `CFBundleIconName` override |
| Resource phase | only `Images.xcassets` and `LaunchScreen.storyboard` |
| Build scripts | RN/Pods bundling only; nothing generates or strips icon layers |
| `IPHONEOS_DEPLOYMENT_TARGET` | `16.4` — not the cause. Icon Composer icons back-deploy. |
| First appearance of the catalog | `b4fdb0a` (RN scaffold). Unchanged through `main`. |

Apple compiles the layered effect from an Icon Composer package
(`AppIcon.icon/icon.json` + `Assets/` layers). A PNG-only or empty
`.appiconset` cannot produce it. The empty catalog is the React Native
template from the original mobile scaffold, not a later flatten.

Rejected hypotheses:

- **A commit deleted or flattened a layered icon.** No `.icon` or icon PNG
  was ever committed under `apps/mobile`.
- **Expo prebuild stripped layers.** Later Expo commits touched `app.json`
  and native wrappers, not icon assets.
- **Wrong catalog name.** `AppIcon` is the correct default; the named
  asset has no content.
- **Deployment target too low.** Not a blocker for compiling a `.icon`.

Unverified (treat as hypothesis only): a local-only `.icon` may have
existed on a Mac and never been committed. Nothing in this clone, GitHub
history, or ignored paths supports that.

## What Apple / Expo require

- An Icon Composer document: directory package `Name.icon/` with
  `icon.json` and `Assets/` (SVG or PNG layers). Background fill,
  groups, lighting, translucency, specular, and glass live in
  `icon.json`.
- Xcode 26+ (`actool`) compiles that package into `Assets.car`. Avoid
  Xcode 26.5 (`actool` nil-object crash); use 26.4.1, 26.6+, or 27.
- Target **App Icon** name must match the package basename (`AppIcon`
  for `AppIcon.icon`).
- Expo SDK 54+ (this app is SDK 57) reads `expo.ios.icon` pointing at a
  `.icon` directory. The committed `ios/` tree is a bare project, so the
  package must also be a target resource, or `npx expo prebuild` must be
  re-run after setting `ios.icon`.
- Keep a 1024×1024 catalog PNG as a fallback. Apple still documents
  inconsistent back-deploy rendering of Icon Composer icons on older iOS
  (radar 152258860).

A working example of this layout already exists in Shay’s
[`paper-qc`](https://github.com/tsonubin/paper-qc) app
(`ios/Paper QC/AppIcon.icon/`). Do not copy that artwork here.

## Missing assets (must be supplied)

Create in Icon Composer (or Xcode 27: File → New → Icon Composer Icon):

```
apps/mobile/assets/AppIcon.icon/
  icon.json
  Assets/
    <foreground layers as SVG or PNG, 1024 canvas, no baked shadows/glass>
```

Optional but recommended for older iOS / App Store:

```
apps/mobile/ios/OnAirMobile/Images.xcassets/AppIcon.appiconset/App-Icon-1024x1024@1x.png
```

(or whatever filename is listed in that set’s `Contents.json`)

Layer guidelines (Apple): separate colors/text/graphics; convert text to
outlines; export opaque SVG/PNG; apply glass, blur, shadows, and
translucency in Icon Composer, not in the source files.

## Exact restore steps (once the `.icon` exists)

1. **Commit the package** at `apps/mobile/assets/AppIcon.icon/` (source of
   truth for Expo) and copy it to
   `apps/mobile/ios/OnAirMobile/AppIcon.icon/` for the bare Xcode target.

2. **`apps/mobile/app.json`** — add under `expo.ios`:

   ```json
   "icon": "./assets/AppIcon.icon"
   ```

3. **`apps/mobile/ios/OnAirMobile.xcodeproj/project.pbxproj`** (bare
   project; objectVersion 54, explicit file list — unlike `paper-qc`’s
   synchronized groups):

   - Add a `PBXFileReference` for `OnAirMobile/AppIcon.icon`
     (`lastKnownFileType = folder.iconcomposer`).
   - Add it to the `OnAirMobile` group and the Resources build phase.
   - Keep `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon` (already set).
   - Optionally set `ASSETCATALOG_COMPILER_INCLUDE_ALL_APPICON_ASSETS = YES`.

4. **Do not** point `ASSETCATALOG_COMPILER_APPICON_NAME` at `AppIcon.icon`
   (include the extension) or at a different basename.

5. **Fallback catalog:** add a 1024×1024 `ios-marketing` PNG to
   `AppIcon.appiconset` and a matching `filename` in `Contents.json`. Do
   not delete the `.appiconset` until older-iOS rendering is confirmed.

6. **Build** a simulator or device install with Xcode 26.4.1 / 26.6+ / 27
   (not Expo Go). Home-screen depth only appears on a build that compiled
   the `.icon` into `Assets.car`.

## How to verify

1. `ls apps/mobile/ios/OnAirMobile/AppIcon.icon/icon.json` and
   `ls apps/mobile/ios/OnAirMobile/AppIcon.icon/Assets/`.
2. Open the iOS target in Xcode → General → App Icons → **AppIcon**.
3. Install on an iOS 26/27 Simulator or device. Confirm the home-screen
   icon has layered lighting / glass (compare Default, Dark, and Tint
   appearances).
4. Inspect the built app: `Assets.car` should contain Icon Composer
   renditions, not only a flat 1024 PNG.
5. Cold-launch on an iOS 16.4–18 device if you still support those
   targets; confirm the catalog fallback is not blank.

## Out of scope

Desktop Tauri icons (`apps/desktop/src-tauri/icons/`) are flattened
PNG/ICNS/ICO and do not affect the iOS home-screen icon. Android
`mipmap-*` launchers are the RN defaults and are unrelated.
