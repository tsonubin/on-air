# Phone Remote on iPhone Duo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the phone remote to Expo SDK 58 and show the mixer and the equalizer side by side on an unfolded iPhone Duo (and any window of at least 600 × 500 pt), then ship 1.0.0 to TestFlight.

**Architecture:**
- **Upgrade first:** SDK 58 lands on its own, in the JS dependencies and the checked-in `ios/` and `android/` projects.
- **Two-pane switch:** a pure `isTwoPane(width, height)` decides the layout from `useWindowDimensions`, so folding, unfolding and split view switch it live.
- **Panes:** `MixerHome` takes an optional `eqPane`. When two-pane, it renders the mixer column and the pane side by side on one faceplate, keeping the mixer's scroll view at the same place in the tree so the volume fader isn't remounted. `EqPane` wraps the existing `EqPanel` with a caption and a Reset key.

**Tech Stack:** Expo SDK 58 (`expo@58.0.17`, React Native 0.88.0, `expo-router@58.0.25`, `@expo/ui@58.0.22`), Jest 29 with `react-test-renderer`, Biome, Xcode 27.1 (the iPhone Duo simulator type and the iOS 27.1 runtime are installed), EAS CLI (logged in as `tsonubin`).

**Spec:** `docs/superpowers/specs/2026-10-22-mobile-iphone-duo-design.md`

## Global Constraints

- **Two-pane rule:** `twoPane = width >= 600 && height >= 500` (window points). The tokens are `layout.twoPaneMinWidth: 600` and `layout.twoPaneMinHeight: 500`.
- **Below that,** today's layouts are unchanged: phone portrait; the landscape phone keeps its two-column mixer at ≥ 700 pt wide.
- **EQ key:** removed (not hidden) while the EQ pane shows. The other three keys stay.
- **No new state:** the pane uses the session's `gains`, `changeBand`, `applyEq` and `view.controlsEnabled`, with the same commit timing (`useFaderDraft`).
- **Side padding:** custom screens pad by `Math.max(insets.left | insets.right, their current padding)`.
- **Dependencies:** none beyond what `expo install --fix` brings for SDK 58. Native template changes follow Expo's 57 → 58 upgrade helper.
- **Versioning:** version `1.0.0` in `app.json` `expo.version`, the mobile `package.json`, `MARKETING_VERSION` (both configurations in `ios/OnAirMobile.xcodeproj/project.pbxproj`) and Android `versionName`. The build number auto-increments from 4 to 5.
- **Commands** run from `apps/mobile` unless shown otherwise. Gates: `pnpm test`, `pnpm typecheck`, and from the repo root `pnpm check:ci` and `pnpm test:native`.
- **Commits** use conventional `build(mobile)` / `feat(mobile)` / `fix(mobile)` / `docs(mobile)` / `chore(mobile)` messages with no attribution lines.
- **Ports for device checks:** mock core `ON_AIR_MOCK=1 PORT=47995 /Users/shay/Projects/oss/on-air/target/debug/on-air-core`, Metro `CI=1 pnpm exec expo start --port 8089 --clear`. Restart Metro with `--clear` after any `sed -i` edit, because its watcher misses those.

## Review Focus

1. **A fold or unfold mid-drag** sends no stray commit and doesn't crash. The volume fader keeps its place in the tree. (Task 4)
2. **A narrow split view** (e.g. 320 × 900) falls back to the phone layout, with nothing clipped and no EQ pane. (Task 4)
3. **A paused or disconnected desktop** disables every fader and the Reset key in the EQ pane, as it does on the EQ screen. (Task 4)
4. **The largest text in two-pane:** the three-key row wraps when its captions don't fit the measured pane width. (Task 4)
5. **Unfolding while the EQ screen is open** keeps that screen open, and Back returns to the two-pane mixer with no duplicate EQ. (Task 4)

---

### Task 1: Expo SDK 58 upgrade

**Files:**
- Modify:
  - `apps/mobile/package.json`, `pnpm-lock.yaml`
  - `apps/mobile/app.json` (only if the doctor requires it)
  - `apps/mobile/ios/**` (Podfile, Podfile.lock, project and AppDelegate/SceneDelegate per the upgrade helper)
  - `apps/mobile/android/**` (Gradle files and wrapper per the upgrade helper)
  - `apps/mobile/__mocks__/*` and source files where the new types or surfaces require it
- Test: the existing suite

**Interfaces:**
- Produces: the app on SDK 58 / React Native 0.88. Every later task builds on it.

- [ ] **Step 1: Upgrade the JS packages**

```bash
pnpm exec expo install expo@^58.0.0 --fix
pnpm dlx expo-doctor@latest
```

Expected:
- `expo` is ^58.0.17, `react-native` 0.88.x, `expo-router` ~58, `@expo/ui` ~58, and `react-native-safe-area-context` on the version the doctor accepts.
- The doctor reports no failing checks, or only native-project checks that Step 3 fixes.

- [ ] **Step 2: Run the gates and fix what React Native 0.88 and Expo 58 break**

```bash
pnpm test && pnpm typecheck
(cd ../.. && pnpm check:ci)
```

Expected: PASS. Fix the source or the mocks, never the assertions, unless a behavior changed by design in SDK 58. Ledger each such case as a ruling. Known places to check, from the spec:
- strict TypeScript types
- removed `StatusBar` props
- `<Host>` top-aligning content, with forms and sheets keeping `flex: 1`
- unset stack `spacing` becoming `nil`
- `RNHostView` losing `style` on Android
- `expo-router` import paths

- [ ] **Step 3: Apply the native template changes for 57 → 58**

Open Expo's native project upgrade helper for the 57.0.0 → 58.0.0 diff (https://docs.expo.dev/bare/upgrade/), and apply each `ios/` and `android/` change by hand. The SceneDelegate already exists; keep it.

Then:

```bash
(cd ios && LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 pod install)
```

Expected: `Pod installation complete!`. If CocoaPods stalls on the React Native core or Hermes tarballs, pre-download them into `~/Library/Caches/ReactNative/`, then set `RCT_TESTONLY_RNCORE_TARBALL_PATH` and `HERMES_ENGINE_TARBALL_PATH`, as in the retro-theme work.

- [ ] **Step 4: Build and launch on iOS and Android**

- **iOS:**
  - Start the mock core and Metro (Global Constraints).
  - `xcodebuild -workspace ios/OnAirMobile.xcworkspace -scheme OnAirMobile -configuration Debug -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 17 Pro' -derivedDataPath ios/build/dd58 build`
  - Install the `.app` and launch it with `-RCT_jsLocation localhost:8089`.
- **Android:**
  - `(cd android && ./gradlew :app:assembleDebug)`
  - Install the APK on `emulator-5554`.
  - `adb reverse tcp:8089 tcp:8089`, and set the debug host to `localhost:8089` as in the earlier Android pass. Restore the prefs afterwards.

Expected: on both, the app opens to the mixer (or to pairing) with the faceplate drawn, and a volume drag shows up in `on-air status --json --base http://127.0.0.1:47995`.

- [ ] **Step 5: Run every gate**

```bash
pnpm test && pnpm typecheck
(cd ../.. && pnpm check:ci && pnpm test:native)
```

Expected: all pass.

- [ ] **Step 6: Commit** `build(mobile): upgrade to Expo SDK 58 and React Native 0.88`

---

### Task 2: The two-pane rule

**Files:**
- Modify: `src/theme.ts` (adds `layout.twoPaneMinWidth`, `layout.twoPaneMinHeight`)
- Create: `src/two-pane.ts`
- Test: `__tests__/two-pane.test.ts`, `__tests__/theme.test.ts`

**Interfaces:**
- Produces: `isTwoPane(width: number, height: number): boolean`, and `layout.twoPaneMinWidth === 600`, `layout.twoPaneMinHeight === 500`.

- [ ] **Step 1: Write the failing tests**

```ts
test("two panes from 600 × 500 pt: unfolded Duo, iPads, foldables", () => {
  expect(isTwoPane(599, 900)).toBe(false); // just under
  expect(isTwoPane(600, 900)).toBe(true);
  expect(isTwoPane(669, 951)).toBe(true); // Duo unfolded, upright
  expect(isTwoPane(951, 669)).toBe(true); // Duo unfolded, sideways
  expect(isTwoPane(900, 499)).toBe(false); // too short for the EQ
  expect(isTwoPane(844, 390)).toBe(false); // phone landscape keeps its layout
  expect(isTwoPane(375, 812)).toBe(false); // Duo folded / phones
  expect(isTwoPane(320, 900)).toBe(false); // narrow split view
});
```

In `theme.test.ts`, add `expect(layout).toMatchObject({ twoPaneMinWidth: 600, twoPaneMinHeight: 500 })`.

- [ ] **Step 2: Run them and confirm they fail.** Run `pnpm test -- two-pane theme`. Expected: FAIL, because the module doesn't exist.
- [ ] **Step 3: Implement the tokens and `isTwoPane`**, comparing with `>=` against the two tokens.
- [ ] **Step 4: Run them and confirm they pass.** Same command. Expected: PASS.
- [ ] **Step 5: Commit** `feat(mobile): a two-pane rule for unfolded and tablet windows`

---

### Task 3: `EqPane`

**Files:**
- Modify: `src/eq-panel.tsx` (adds `EqPane`)
- Test: `__tests__/eq-panel.test.tsx`

**Interfaces:**
- Consumes: `EqPanel` (existing), `PanelKey` (existing, `src/ui/panel-key.tsx`).
- Produces: `EqPane({ gains: number[]; disabled: boolean; onChangeBand: (index: number, value: number) => void; onReset: () => void })`.
  - It renders a `View` with `testID="eq-pane"`.
  - The header row has a `Text` caption `Equalizer` (`type.label`, `accessibilityRole="header"`, `maxFontSizeMultiplier={1.3}`) and a `PanelKey` labelled `Reset` (`testID="eq-pane-reset"`, `accessibilityLabel="Reset equalizer"`).
  - The `EqPanel` comes below.

- [ ] **Step 1: Write the failing tests**
  - **"the pane names itself and holds the five faders":**
    - `eq-pane` exists.
    - A `Text` with children `Equalizer` has `accessibilityRole: "header"`.
    - `eq-band-0` through `eq-band-4` exist inside `eq-pane`.
  - **"Reset is off while flat or disabled, and flattens the curve":**
    - gains `[0,0,0,0,0]` gives `eq-pane-reset` with `onPress` undefined.
    - gains `[2,0,0,0,0]` with `disabled` gives `onPress` undefined.
    - gains `[2,0,0,0,0]` without `disabled`: pressing calls `onReset` once.
  - **"a disabled pane disables every band"** (Review Focus 3): with `disabled`, every `eq-band-{i}` has `accessibilityState` `{ disabled: true }`.
- [ ] **Step 2: Run them and confirm they fail.** Run `pnpm test -- eq-panel`. Expected: FAIL, because `EqPane` isn't exported.
- [ ] **Step 3: Implement `EqPane`.** It is a `View` with `flex: 1` and `gap: 14`. The header row is `flexDirection: "row"`, `alignItems: "center"`, `justifyContent: "space-between"`. The key's `style` is `{ minWidth: 88 }`.
- [ ] **Step 4: Run them and confirm they pass.** Same command. Expected: PASS.
- [ ] **Step 5: Commit** `feat(mobile): an equalizer pane with its own Reset`

---

### Task 4: Mixer + EQ side by side

**Files:**
- Modify:
  - `src/mixer-home.tsx`: the `eqPane` prop, the two-pane tree, the three-key row, and `keysFitInRow` gaining a key count
  - `app/index.tsx`: builds the `EqPane`
- Test: `__tests__/App.test.tsx`

**Interfaces:**
- Consumes: `isTwoPane` and the tokens (Task 2), `EqPane` (Task 3).
- `MixerHome` adds the prop `eqPane?: React.ReactNode`.
- `keysFitInRow(rowWidth: number, fontScale: number, keys = 4): boolean`. The gap count becomes `keys - 1` and the key width `(rowWidth - (keys - 1) * KEY_GAP) / keys`.
- **The tree:** `Faceplate(mixer-screen)` › `Header` › `View testID="mixer-panes"` (`flexDirection: "row"`, `flex: 1`) › the mixer `ScrollView` (always the first child, `flex: 1`) › [when two-pane: a vertical groove `View testID="pane-fold"` (2 pt wide, the same `grooveDark` / `grooveLight` edges as `Groove`, hidden from accessibility), then a `ScrollView testID="eq-pane-scroll"` (`flex: 1`, `contentContainerStyle={{ flexGrow: 1, padding }}`) holding `eqPane`].
- **The inner layout** testID is `mixer-pane-layout` when two-pane. Otherwise it stays as today (`mixer-phone-layout` / `mixer-wide-layout` / `mixer-landscape-layout`).
- **Two-pane column:** the mixer always uses the single-column order (no wide row).
- `index.tsx` passes `eqPane={<EqPane gains={remote.gains} disabled={!view.controlsEnabled} onChangeBand={remote.changeBand} onReset={() => remote.applyEq([0, 0, 0, 0, 0])} />}`.

- [ ] **Step 1: Update the existing layout test to the spec's table**

The old "layouts: two panes when unfolded, compact rows in phone landscape" test becomes:
- At 852 × 883: `mixer-panes` contains `eq-pane`, `mixer-pane-layout` exists, and `eq-key` does not exist.
- At 844 × 390: `mixer-landscape-layout` exists and `eq-pane` does not.
- In both cases `source-key`, `speaker-key`, `volume-slider` and `format-key` exist.

- [ ] **Step 2: Write the failing new tests** (helpers as in `App.test.tsx`; set the window with `jest.mocked(useWindowDimensions).mockReturnValue(...)`, then `ReactTestRenderer.act(() => tree?.update(<RootLayout />))`):
  - **"unfolding shows the EQ beside the mixer; folding puts it away":**
    - At 375 × 812: `eq-pane` doesn't exist and `eq-key` does.
    - Update to 669 × 951: `eq-pane` exists and `eq-key` doesn't.
    - Back to 375 × 812: as before.
  - **"the EQ pane writes through the same session calls":**
    - At 669 × 951 with fake timers, run `stepFader("eq-band-2", "increment")` and advance 180.
    - `api.setEq` is last called with `[0, 0, 1, 0, 0]`.
    - Pressing `eq-pane-reset` after that calls `api.setEq` with `[0, 0, 0, 0, 0]`.
  - **Review Focus 1, "a fold mid-drag sends no stray commit and keeps the volume fader":**
    - At 669 × 951, keep `const fader = host("volume-slider")`, call `onResponderGrant(touch(0))`, then update the window to 375 × 812.
    - `host("volume-slider")` is the same instance as `fader`.
    - Advance 500; `api.setVolume` is not called.
  - **Review Focus 2, "a narrow split view falls back to the phone layout":** 320 × 900 gives `mixer-phone-layout`, no `eq-pane`, and `eq-key` present.
  - **Review Focus 3, "a paused desktop disables the EQ pane":** at 669 × 951, with the paused setup from the existing paused test, `eq-band-0` has `accessibilityState` `{ disabled: true }` and `eq-pane-reset` has `onPress` undefined.
  - **Review Focus 4, "the three-key row wraps at the largest text in a pane":** at 669 × 951 with `fontScale: 1.6`, fire `onLayout` on `mixer-keys` with width 290. `source-key` has `flexBasis: "48%"`. Add a unit check that `keysFitInRow(290, 1, 3)` is true and `keysFitInRow(200, 1, 3)` is false.
  - **Review Focus 5, "unfolding while the EQ screen is open keeps it":**
    - At 375 × 812, press `eq-key`, then update to 669 × 951.
    - `equalizer-screen` exists and `eq-pane` does not, since the router mock renders only the top screen.
    - `native-back` returns to `mixer-screen`, where `eq-pane` exists.
- [ ] **Step 3: Run them and confirm they fail.** Run `pnpm test -- App`. Expected: FAIL, because there is no `eq-pane` and no `mixer-panes`.
- [ ] **Step 4: Implement** the tree and props above, and the `keysFitInRow` key count. The key row passes `twoPane ? 3 : 4` keys.
- [ ] **Step 5: Run them and confirm they pass.** Run `pnpm test && pnpm typecheck`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(mobile): mixer and equalizer side by side on wide windows`

---

### Task 5: Side safe areas on the custom screens

**Files:**
- Modify: `src/mixer-home.tsx` (the header row and the mixer and pane scroll padding), `src/pairing-home.tsx` (scroll padding)
- Test: `__tests__/App.test.tsx`

**Interfaces:**
- Consumes: `useSafeAreaInsets` (already a `jest.fn` in `__mocks__/safe-area-context.js`).

- [ ] **Step 1: Write the failing tests**
  - **"the mixer keeps clear of a side rail":**
    - `jest.mocked(useSafeAreaInsets).mockReturnValue({ top: 0, right: 59, bottom: 21, left: 59 })`, then render and pair.
    - The flattened `contentContainerStyle` of the mixer `ScrollView` (the parent of `mixer-phone-layout`) has `paddingLeft` and `paddingRight` of 59.
  - **"pairing keeps clear of a side rail":** with the same insets, the `pairing-scroll` `contentContainerStyle` has `paddingLeft` and `paddingRight` of 59.
- [ ] **Step 2: Run them and confirm they fail.** Run `pnpm test -- App`. Expected: FAIL; they're still 20.
- [ ] **Step 3: Implement.** Replace `paddingHorizontal: compact ? 16 : 20` with `paddingLeft: Math.max(insets.left, pad)` and `paddingRight: Math.max(insets.right, pad)`. Do the same for the header row (base 20) and the EQ pane scroll.
- [ ] **Step 4: Run them and confirm they pass.** Run `pnpm test`. Expected: PASS.
- [ ] **Step 5: Commit** `fix(mobile): keep the mixer and pairing clear of side safe areas`

---

### Task 6: Device pass and version 1.0.0

**Files:**
- Modify:
  - `apps/mobile/app.json`, `apps/mobile/package.json`
  - `apps/mobile/ios/OnAirMobile.xcodeproj/project.pbxproj` (`MARKETING_VERSION` ×2)
  - `apps/mobile/android/app/build.gradle` (`versionName`)
  - `CHANGELOG.md`
  - the spec, for the measured Duo points

- [ ] **Step 1: Set up the Duo and launch on it**
  - Create and boot an iPhone Duo: `xcrun simctl create "iPhone Duo" com.apple.CoreSimulator.SimDeviceType.iPhone-Duo com.apple.CoreSimulator.SimRuntime.iOS-27-1`, then `xcrun simctl boot <udid>`.
  - Build for it, as in Task 1 Step 4 with `-destination 'platform=iOS Simulator,id=<udid>'`.
  - Install and launch it against Metro :8089 and the mock :47995.
- [ ] **Step 2: Check the screens**
  - **Unfolded:**
    - The mixer and EQ show side by side, with the fold groove centered.
    - Record the window size (`Dimensions.get("window")` in a dev log, or the screenshot's point size) in the spec's facts.
    - Drag the volume and one EQ band in the pane; `on-air status --json` shows both.
    - Pane Reset leaves the EQ flat.
  - **Folded:** use Device Hub's fold control. If folding can't be scripted, use the iPhone 17 Pro simulator as the stand-in for the outer screen and record that ruling. Folded shows the phone layout with the EQ key.
  - **Largest text size:** cold launch, then check the key wrap and that the EQ pane fits.
  - **Regressions:** the iPhone 17 Pro and Android emulator screens from Task 1 still look right.
- [ ] **Step 3: Fix what the device pass finds, test-first.** Each finding is a failing test, then a fix, then a passing test, then a commit.
- [ ] **Step 4: Bump the version to 1.0.0 in all five places, and add the CHANGELOG `[Unreleased]` entry**

> Phone remote 1.0: iPhone Duo support (mixer and EQ side by side when unfolded) on Expo SDK 58.

- [ ] **Step 5: Run every gate.** Same as Task 1 Step 5. Expected: all pass.
- [ ] **Step 6: Commit** `chore(mobile): version 1.0.0`

---

### Task 7: Release (after the final review is resolved)

- [ ] **Step 1: Merge to `main` and push**

```bash
git fetch origin
git switch main 2>/dev/null || git switch -c main origin/main
git merge --no-ff feat/mobile-iphone-duo -m "Merge the iPhone Duo layout and Expo SDK 58 upgrade"
git push origin main
```

This worktree checks out its own branches, so if `main` is checked out elsewhere, merge with `git push origin feat/mobile-iphone-duo:main` after confirming `origin/main` is an ancestor (a fast-forward), or merge into a temporary branch.

Expected: the push succeeds, and CI on `main` starts.

- [ ] **Step 2: Build and submit to TestFlight**

```bash
cd apps/mobile
npx --yes eas-cli@latest build --platform ios --profile production --auto-submit --non-interactive
```

Expected:
- EAS bumps the build number from 4 to 5 locally.
- The build finishes, and the submit uploads build 1.0.0 (5) to App Store Connect app 6812577609.

If the submit fails for want of an App Store Connect key, stop and report it. The owner then supplies the key (`scripts/publish-testflight.sh`, stage 2), and the upload is retried with `npx eas-cli@latest submit -p ios --latest`.

- [ ] **Step 3: Commit the build-number bump EAS made and push it**

```bash
git add apps/mobile/ios/OnAirMobile.xcodeproj/project.pbxproj apps/mobile/app.json
git commit -m "chore(mobile): build 5 for TestFlight"
git push origin main
```
