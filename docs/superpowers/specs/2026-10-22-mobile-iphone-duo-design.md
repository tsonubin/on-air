# Phone remote on iPhone Duo (Expo SDK 58)

Date: 2026-10-22
Status: design approved in conversation, pending spec review
Builds on: the LED faceplate redesign (`2026-10-22-mobile-led-faceplate-redesign-design.md`, merged to `main` in #33)
Branch: `feat/mobile-iphone-duo`, from `main` (bff4f1f)

## Goal

The on-air phone remote works well on the iPhone Duo, Apple's first foldable.
It went on sale on October 23, 2026, with a 7.6" inner display and a 5.4"
outer one.

- **Unfolded:** the mixer and the equalizer sit side by side, so you can ride
  the volume and the tone without switching screens.
- **Folded:** the outer screen keeps today's phone layout.
- **Release:** the result ships as version 1.0.0 to TestFlight.

Success means:

- The app builds and runs on Expo SDK 58 (React Native 0.88), on iOS 27 and on
  Android, with every gate green.
- On an unfolded Duo, the mixer and the EQ show together. Folding, unfolding
  and split view switch the layout live, with no restart and no lost drag.
- Nothing on the folded Duo or any current phone changes, except what SDK 58
  itself changes.
- Build 1.0.0 (5) is uploaded to TestFlight.

## Decisions

- **The owner chose, in conversation:**
  - Mixer + EQ side by side when unfolded.
  - Approach A, a React Native two-pane layout.
  - Upgrade to Expo SDK 58 first.
  - Push to `main` when done.
  - A new major version, 1.0.0, on TestFlight.
- **Why not `ArrangementView`:** Expo UI's `ArrangementView` (Apple's
  recommended SwiftUI container for the Duo) is iOS-only, nests React Native
  content inside a SwiftUI host (which this app avoids), and risks touch
  conflicts with the custom faders.
- **Why two-pane works without it:** the Duo's fold is the vertical center
  line of the inner display, which is where the two panes meet anyway.

Facts this rests on:

- **Expo SDK 58** (October 15, 2026) is built for iOS 27, includes React
  Native 0.88, and supports the Duo out of the box. Apps must:
  - respond to window size changes when the phone folds or enters split view
  - expect bars and controls in a vertical rail on the left or right edge
  - handle horizontal safe areas in more places

  `requireFullScreen` and `ScreenOrientation.lockAsync` no longer stop
  resizing on iOS 27. (Source: <https://expo.dev/changelog/sdk-58>.)
- **Inner display width:** about 626–669 pt wide upright and 890–951 pt
  sideways. Apple publishes 1878 × 2670 px; simulator reports differ. The
  Duo simulator is the authority, so it is measured during the device pass.
- **Outer display width:** phone-sized, under 450 pt.

## Scope

**In scope:**

- the SDK 58 upgrade of `apps/mobile` and its checked-in `ios/` and
  `android/` projects
- the two-pane mixer + EQ layout
- side safe areas on the custom screens
- the 1.0.0 version bump
- the TestFlight build and upload

**Out of scope:**

- hinge-angle effects (`expo-hinge`: Apple recommends hinge state for
  interactions, not layout)
- iPad- or Duo-specific changes to pairing, Audio format and Connection,
  beyond what SDK 58 and the safe areas require
- Android release builds
- the desktop app and the core

## Part 1: Expo SDK 58 upgrade (first, its own commits)

- Run `pnpm exec expo install expo@^58.0.0 --fix`, then
  `pnpm dlx expo-doctor@latest`, and fix what it reports.
- Every Expo package moves to its 58 version: expo-router, `@expo/ui`,
  expo-font, expo-secure-store, expo-network, expo-status-bar and the rest.
  React Native moves to 0.88, with matching React,
  `react-native-safe-area-context`, `react-native-screens` and Reanimated.
- **React Native 0.88:**
  - Fix type errors from the strict TypeScript API.
  - Check the removed APIs against our code: `StatusBar` color props,
    `Touchable`, `InteractionManager`, `react-native/Libraries/*` deep
    imports.
  - Check that `experimental_backgroundImage`, `boxShadow` and `PanResponder`
    still behave. They are not listed as changed.
- **Expo UI 58:**
  - `<Host>` top-aligns its content, so check every settings form, sheet and
    icon host.
  - Unset `spacing` in stacks becomes `nil`; pass the old spacing where
    layouts shift.
  - On Android, `style` is gone from `RNHostView`.
- **Expo Router 58:** check our `ThemeProvider`, `Stack`, `Stack.Toolbar` and
  `Stack.Protected` imports against the reworked navigation core.
  `Stack.Toolbar` on Android now needs `@expo/ui`, which we already depend on.
- **Native projects:**
  - iOS: `pod install`, using the cached tarball workarounds from the
    retro-theme work if CocoaPods stalls. SDK 58 needs the UIScene
    `SceneDelegate`, which the app already has.
  - Android: a Gradle sync and a debug build.
  - The project versions the upgrade helper lists for SDK 58 are applied by
    hand where `--fix` does not reach the native files.
- **Test mocks:** update `__mocks__/` wherever a package's surface changed.
- **Done when:** the full gates pass (`pnpm test`, `pnpm typecheck`,
  `pnpm check:ci`, `pnpm test:native`) and the app launches on the iOS 27
  simulator and on the Android emulator against the mock core.

## Part 2: The two-pane layout

### When it shows

`twoPane = width >= 600 && height >= 500` (window points, read from
`useWindowDimensions`, so it updates live on fold, unfold, rotation and split
view).

| Device / state | Window (pt, approx.) | Layout |
| --- | --- | --- |
| Phones portrait, Duo folded | ≤ 450 wide | Phone (unchanged) |
| Phones landscape | ~850 × 390 | Today's two-column mixer, no EQ pane (unchanged) |
| Duo unfolded upright | ~626–669 × 890–951 | Mixer + EQ |
| Duo unfolded sideways | ~890–951 × 626–669 | Mixer + EQ |
| iPad, Android foldables / tablets | ≥ 600 × ≥ 500 | Mixer + EQ |

The 500 pt height floor keeps landscape phones on today's layout, where the
EQ would not fit. The mixer's own two-column mode (window + controls at 700
pt and up) stays only for the landscape-phone case. Every taller window that
wide becomes two-pane.

New layout tokens in `src/theme.ts`: `layout.twoPaneMinWidth: 600` and
`layout.twoPaneMinHeight: 500`.

### What it shows

- **One faceplate:** the header row (sign and ⋯) spans the top.
- **Left pane:** the mixer as a single column, with the same order as the
  phone layout (window, messages, CD, groove, volume, groove, keys).
- **Divider:** a vertical `Groove` at the center, on the Duo's fold.
- **Right pane:** an `EqPane`, which is the existing `EqPanel` plus a top row
  with a silkscreen `EQUALIZER` caption and a `RESET` `PanelKey`. Reset keeps
  today's disabled rule: flat, or controls disabled.
- **Pane widths:** equal, each scrolling independently if its content is
  taller than the window.
- **EQ key:** hidden in two-pane, since the EQ is on screen. The other keys
  stay, so the key row becomes three equal keys.
- **The EQ screen** stays reachable from routes (it is never pushed in
  two-pane). If the window grows while it is open, it stays open; Back
  returns to the two-pane mixer.

### Data flow

- `MixerHome` gains an optional `eqPane?: ReactNode` prop. When `twoPane` is
  true and the prop is set, it renders the panes, and it hides the EQ key
  whenever the pane is shown.
- `app/index.tsx` builds the `EqPane` from the session (`gains`,
  `changeBand`, `applyEq`, `view.controlsEnabled`).
- **No new state:** the EQ pane reads and writes through the same session
  calls as the EQ screen. Its faders use `useFaderDraft`, so commit timing and
  the held-finger rule are unchanged.
- `EqPane` lives in `src/eq-panel.tsx` next to `EqPanel`.
- `app/equalizer.tsx` keeps its own header Reset (iOS toolbar, Android
  caption).

### Live resizing

- The layout follows `useWindowDimensions`, with nothing cached at mount.
- A fold or split change during a fader drag must not lose the gesture or
  send a stray commit. The fader's responder and `useFaderDraft` hold already
  survive a re-render. Re-parenting across layouts may remount the volume
  fader, so the volume fader keeps the same key and position in the tree
  across layouts.
- A remount mid-drag ends the gesture without a commit. That is acceptable:
  the last preview has already been committed after its pause, or it is
  dropped, as with a lifted finger.

### Edges and the rail

The mixer, pairing and EQ screens pad by
`max(insets.left|right, their current padding)`. On the Duo, iOS puts system
bars in a vertical rail at one edge, and the side insets keep our content
clear of it. The native-header screens already follow the system.

## Part 3: Release

- **Merge:** the branch merges to `main` with a merge commit and is pushed.
- **Version: 0.2.0 → 1.0.0** in:
  - `apps/mobile/app.json` (`expo.version`)
  - `apps/mobile/package.json`
  - `ios/OnAirMobile.xcodeproj/project.pbxproj` (`MARKETING_VERSION`, both
    configurations)
  - `android/app/build.gradle` (`versionName`)
- **Build number:** 4 → 5. EAS's production profile auto-increments it
  (`autoIncrement: true`, `appVersionSource: local`). The bump is committed
  after the build.
- **CHANGELOG:** an `[Unreleased]` entry: "Phone remote 1.0: iPhone Duo
  support (mixer and EQ side by side when unfolded) on Expo SDK 58."
- **Upload:** from `apps/mobile`, run
  `eas build --platform ios --profile production --auto-submit`.
- **Credential risk:** the App Store Connect key that
  `scripts/publish-testflight.sh` stored is not on this Mac
  (`~/.eas/on-air.env` is missing). If EAS has no stored submit key, the
  submit step stops. The owner then re-runs that script's stage 2, or
  provides the key to EAS, and the submit is retried with
  `eas submit -p ios --latest`.

## Error handling

- **No new failure paths.** The EQ pane shows the same errors as the mixer,
  under the LED window.
- **A paused or disconnected desktop** disables the EQ pane exactly as it
  disables the EQ screen (`controlsEnabled`).

## Accessibility

- In two-pane, VoiceOver and TalkBack reach the mixer pane first, then the EQ
  pane, which is announced as "Equalizer" (an accessible header on its
  caption).
- The hidden EQ key is removed, not just made invisible, so it is no
  screen-reader stop.
- The text-scale caps carry over. At the largest sizes the key row wraps as
  it does now.

## Testing

TDD: every behavior gets a test that fails first.

**Unit tests:**

- `twoPane` is false at 599 × 900, true at 600 × 900, and false at 900 × 499.
- At 600 × 900 the mixer renders the EQ pane (`eq-pane`) with five faders,
  hides `eq-key`, and keeps `source-key`, `speaker-key` and `format-key`.
- Pane Reset follows the flat and disabled rules and calls `applyEq` with
  zeros.
- A drag in the pane commits through `changeBand`.
- Re-rendering from 390 × 844 to 669 × 951 (unfold) switches layouts with no
  commit sent, and back again (fold).
- Side insets of 59 pt pad the mixer and pairing screens.
- The existing layout tests are updated to the new table.

**Gates:** `pnpm test`, `pnpm typecheck`, `pnpm check:ci` and
`pnpm test:native`.

**Device pass** on the iPhone Duo simulator (Xcode 27.1 / Device Hub) against
the mock core:

- Folded: the phone layout.
- Unfolded: mixer + EQ. Measure the real point size and record it in this
  spec.
- Fold and unfold mid-session.
- Split view.
- Drag volume and an EQ band in the pane, and confirm both with
  `on-air status --json`.
- Pane Reset.
- The largest text size.
- Then check SDK 58 on the iPhone 17 Pro simulator and the Android emulator
  for regressions.
