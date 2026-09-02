# Mobile redesign QA

## Visual target and capture state

- Source: `/Users/shay/.codex/generated_images/01a05ccd-6803-77c2-a285-28289c4e629e/exec-fafa3459-0e0b-4e02-91dc-bbdc1908230d.png`
- Source dimensions: 853 × 1844 px, normalized to 390 × 844 px for comparison.
- iOS implementation: `/tmp/on-air-mobile-redesign/ios-main-final-v3-1170x2532.png`
- iOS viewport: 390 × 844 pt at 3× density (1170 × 2532 px), dark appearance.
- Android standalone build: `1e5c1e84-d62a-4615-8df4-c664dde3b713` (`preview`, APK, Expo SDK 57).
- Android implementation: `/tmp/on-air-native-latest-phone-portrait.png`
- Android normal-phone viewport: 443 × 970 dp at 390 dpi (1080 × 2364 px), Pixel 10 Pro Fold outer display.
- First-run pairing after discovery: `/tmp/on-air-native-latest-pairing-selected.png`
- Fold open, responsive two-pane layout: `/tmp/on-air-native-latest-fold-open.png`
- Fold half-open, landscape/tabletop layout: `/tmp/on-air-native-latest-fold-half-open.png`
- Outer display, normal-phone portrait layout: `/tmp/on-air-native-latest-phone-portrait.png`
- Outer display, normal-phone landscape layout: `/tmp/on-air-native-latest-phone-landscape.png`
- Fold half-open, dark sound sheet: `/tmp/on-air-native-latest-sound-sheet.png`
- Fold layout before/after comparison: `/tmp/on-air-fold-before-after.png`
- State: paired with the mock core; Mock Monitor routes to Mock Sonos; audio is live; volume was changed from 50 to 80 and independently read back as 80 from the desktop API.
- The floating blue gear visible in Expo Go captures is development-runtime chrome. It is not rendered by the app and is excluded from app-owned visual findings.

## Comparison inputs

- Full source and iOS implementation: `/tmp/on-air-mobile-redesign/comparison-main-v3-full.png`
- Header and route focus: `/tmp/on-air-mobile-redesign/comparison-main-v3-header-route.png`
- Live, volume, settings, and exit focus: `/tmp/on-air-mobile-redesign/comparison-main-v3-controls.png`
- iOS compact sound sheet: `/var/folders/mg/xfgz90gs6799jh7xvxs356g80000gn/T/screenshot_optimized_3d7b7ed8-c68b-4c6c-bca2-ae79f001edf9.jpg`
- iOS expanded equalizer: `/var/folders/mg/xfgz90gs6799jh7xvxs356g80000gn/T/screenshot_optimized_5f813168-94f9-4488-a731-2c41e6eec7b9.jpg`
- Android dark sound sheet: `/tmp/on-air-mobile-redesign/android-dark-sound-sheet.png`

Both focused comparisons were required because the full 390 × 844 comparison makes the native slider thumb, route indicators, and bottom action copy harder to judge precisely.

## Iteration history

1. **P1 — Android routed screen was blank.** A React Native screen had been wrapped in `RNHostView`, which is not needed for this cross-platform root. Removed the wrapper and verified the live mixer tree and rendered screen on Android.
2. **P1 — Android volume control was visually noisy and partially unreadable.** The stepped Material slider drew a dense field of tick marks and native labels used the wrong foreground color. Changed the slider to continuous native interaction with integer quantization, moved the visible label/value to a shared React Native header, and verified a 50 → 69 → 50 gesture loop.
3. **P1 — Sheets did not preserve the dark visual system and initially used an oversized presentation.** Forced the app to dark appearance, added compact iOS detents for scoped tasks, and retained full-height expansion for the equalizer. Verified compact and expanded sheets on iOS and a dark Material sheet on Android.
4. **P2 — Keyboard flow could strand the pairing code below the keyboard.** Added explicit host-field submission focus to the PIN field and retained PIN submission as the pairing action.
5. **P2 — App content inherited a redundant native safe-area offset.** Made the root native host ignore its own safe area because the React Native header and scroll content already apply safe-area insets. The final source/implementation comparison now aligns the route, live state, volume card, settings row, and bottom action within a few points.
6. **P2 — Desktop-style volume increment/decrement actions were still represented in tests and component API.** Removed both controls, removed their API, updated tests, and kept only the platform-native slider.
7. **P1 — The open-fold layout stretched the phone stack across the inner display.** Added a responsive two-pane mixer composition: route and live state stay together on the left, while volume, sound settings, and disconnect remain grouped on the right.
8. **P1 — Android could render a blank native host after disconnecting.** Keyed the native host by restoration/pairing/mixer state so crossing the authentication boundary remounts the host and reliably returns to discovery.
9. **P2 — Phone landscape pushed important controls too far below the first viewport.** Added a compact short-wide mode with denser route rows, a reduced live-state footprint, and tighter vertical rhythm while preserving scrolling and touch-target sizes.
10. **P2 — The app was locked to portrait despite fold posture changes.** Enabled system-driven orientation and added iPhone landscape declarations while retaining the single-column phone portrait layout.
11. **P1 — Pairing still read as a form-heavy setup page.** Replaced it with progressive first-run discovery: search first, show a selectable Mac when found, reveal the code panel only after selection, and keep the LAN address behind “Set up manually.” Pairing is securely persisted before the mixer opens.

## Final findings

- P0: none.
- P1: none.
- P2: none.
- The implementation intentionally uses the platform-native white iOS slider thumb and a semantic speaker-volume icon instead of imitating the concept art's red custom thumb and muted icon.
- The bottom action says “Disconnect remote” because the current backend exposes remote disconnect, not a stop-streaming endpoint. This avoids presenting a destructive control that cannot perform its label.
- Route rows, sound settings, source selection, output selection, pairing, disconnect, and the native volume slider are interactive. All primary touch targets are at least 44 pt/dp.
- Pairing is a one-time first-run task unless the user explicitly chooses “Disconnect remote.” A force-stop and cold launch restored the mixer directly with one `mixer-screen`, zero `pairing-screen`, and no repeat discovery or PIN verification in the restart unit test.
- Android unit coverage explicitly checks phone portrait, short-wide phone landscape, and unfolded two-pane rendering. Fold open, half-open/tabletop, and outer-display phone states were also inspected on a Pixel 10 Pro Fold emulator.
- Android foldable-width layout, system bars, compact/dark sheet presentation, disconnect/remount, re-pairing, and slider gesture were checked in the standalone APK with UI-tree-derived coordinates. The Android crash buffer was empty after the final pass.

final result: passed
