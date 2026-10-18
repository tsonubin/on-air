# Mobile Retro Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reskin the Expo phone remote (`apps/mobile`) in the desktop tray app's retro look: desktop palette, Anton ONAIR sign, DSEG14 LED readouts, IBM Plex type, amber-tinted native controls.

**Architecture:** All color and type decisions live in `src/theme.ts` (tokens and type presets) and two new presentational pieces, `Sign` and `LedReadout`. Fonts load at runtime with `expo-font` behind a gate in `app/_layout.tsx`. Native Expo UI surfaces take the palette through `Host seedColor`, SwiftUI form modifiers and a navigation theme; no native project files change.

**Tech Stack:** Expo SDK 57, React Native 0.86, `@expo/ui` 57 (SwiftUI / Jetpack Compose), `expo-font` 57, expo-router native stack, Jest 30 with react-test-renderer.

**Spec:** `docs/superpowers/specs/2026-10-18-mobile-retro-theme-design.md`

## Global Constraints

- Hex values come verbatim from the spec's token table (desktop `apps/desktop/src/App.css`). No other colors.
- Red (`colors.accent`, `#ff3b2a`) only for live state: lit sign, live lamp, `LIVE` status, active-route dot. Amber (`colors.amber`, `#e0a24b`) for values, readouts, control tint, links, the Connect key.
- Only new dependency: `expo-font@~57.0.4` in `apps/mobile/package.json`. No `expo-splash-screen`.
- No changes under `apps/mobile/ios/` or `apps/mobile/android/`.
- Existing `testID`s, accessibility labels, roles and user-facing copy stay. Uppercase is visual only (`textTransform: "uppercase"` in React Native; uppercased strings only in native form section headers, which no test reads).
- Text keeps font scaling; only `Sign` and `LedReadout` cap it with `maxFontSizeMultiplier={1.3}`.
- Sign: Anton, 18 pt, letter spacing 2, 1 pt border, radius 4, padding 1 × 8.
- Radii shrink to `radius = { sm: 4, md: 6, lg: 8, xl: 10, xxl: 12 }`.
- Another session edits `packages/core` in this checkout: stage only the files a task names, never `git add -A`.

## Review Focus

1. **Font loading fails** (bad asset, missing file): the app must still render, in system fonts. Pinned in Task 2.
2. **Large text sizes:** the sign and LED readouts must not grow past their containers. Pinned in Task 3 (`maxFontSizeMultiplier`).
3. **Idle, paused and disabled states** must not borrow live red or active amber: unlit sign and `liveDim` lamp when not live; disabled volume readout and Connect key in muted colors. Pinned in Tasks 5 and 6.
4. **PIN ghost digits** must not read as an entered code: `888888` is only a placeholder, the field keeps its label, length and submit. Pinned in Task 6.
5. **Android default purple:** a `Host` without `seedColor` falls back to the Material baseline. Every host gets amber. Pinned in Task 4.

---

### Task 1: Desktop palette, radii and type presets in the theme

**Files:**
- Modify: `apps/mobile/src/theme.ts`
- Test: `apps/mobile/__tests__/theme.test.ts` (create)

**Interfaces:**
- Produces: `colors` with every token in the spec table (existing names keep their meaning; new: `surfaceRaised`, `borderStrong`, `liveDim`, `amber`, `amberBright`, `key`, `onAmber`, `ledWell`, `ledGhost`, `signLitText`, `signLitWell`, `signLitLine`, `signOffText`, `signOffWell`); `selectedSurface = "#3a221c"`; `disabled = "#5b5246"`; `theme.seedColor = colors.amber`.
- Produces: `fonts` — `{ sign: "Anton-Regular", led: "DSEG14ClassicMini-Regular", mono: "IBMPlexMono-Regular", monoMedium: "IBMPlexMono-Medium", sans: "IBMPlexSans-Regular", sansMedium: "IBMPlexSans-Medium", sansSemiBold: "IBMPlexSans-SemiBold" } as const` (PostScript names; Task 2 confirms them).
- Produces: `type: Record<"sign" | "led" | "label" | "title" | "body", TextStyle>` exported from `theme.ts`; `theme.type = type`.

- [ ] **Step 1: Write the failing test** `theme.test.ts`:

```ts
import { colors, fonts, radius, theme, type } from "@/theme";

test("palette matches the desktop tokens", () => {
  expect(colors).toMatchObject({
    background: "#0a0908", surface: "#1a1814", surfaceRaised: "#221f1a",
    surfacePressed: "#2a2218", iconSurface: "#2a261f", border: "#3a342a",
    borderStrong: "#4a4338", separator: "#2e2a24", label: "#f3ead4",
    secondaryLabel: "#c4b89a", tertiaryLabel: "#9a8f78", accent: "#ff3b2a",
    liveDim: "#6a2a22", amber: "#e0a24b", amberBright: "#f0c56a", key: "#5c4a32",
    onAmber: "#141210", ledWell: "#070605", ledGhost: "#1c1812",
    signLitText: "#ff3b2a", signLitWell: "#1a0605", signLitLine: "#8a3a30",
    signOffText: "#5a1814", signOffWell: "#0e0c0a", selectedSurface: "#3a221c",
    errorSurface: "#3a1410", errorBorder: "#6a2a22", onError: "#f0b4ac",
    disabled: "#5b5246", onAccent: "#ffffff",
  });
  expect(theme.seedColor).toBe(colors.amber);
});

test("radii are machined, not pillowy", () => {
  expect(radius).toEqual({ sm: 4, md: 6, lg: 8, xl: 10, xxl: 12 });
});

test("type presets use the vendored families", () => {
  expect(type.sign).toMatchObject({ fontFamily: fonts.sign, fontSize: 18, letterSpacing: 2 });
  expect(type.led.fontFamily).toBe(fonts.led);
  expect(type.label).toMatchObject({ fontFamily: fonts.monoMedium, fontSize: 11, textTransform: "uppercase" });
  expect(type.label.letterSpacing).toBeCloseTo(11 * 0.16);
  expect(type.title.fontFamily).toBe(fonts.sansSemiBold);
  expect(type.body.fontFamily).toBe(fonts.sans);
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm --filter mobile exec jest __tests__/theme.test.ts`
Expected: FAIL (`fonts`/`type` undefined, palette mismatch).

- [ ] **Step 3: Implement in `src/theme.ts`.** Replace the header comment to say the palette mirrors `apps/desktop/src/App.css`. Add `fonts`, the new tokens, `radius`, and `type` (`title` 17 pt, `body` 15 pt, line heights 1.3×; no `fontWeight` on custom families, the family carries the weight).

- [ ] **Step 4: Run the mobile suite**

Run: `pnpm --filter mobile test && pnpm --filter mobile typecheck`
Expected: PASS. Screens still compile because existing token names remain.

- [ ] **Step 5: Commit**

```bash
git add apps/mobile/src/theme.ts apps/mobile/__tests__/theme.test.ts
git commit -m "feat(mobile): desktop palette, radii and type presets"
```

### Task 2: Vendored fonts and the font gate

**Files:**
- Create: `apps/mobile/assets/fonts/{Anton-Regular,DSEG14ClassicMini-Regular,IBMPlexMono-Regular,IBMPlexMono-Medium,IBMPlexSans-Regular,IBMPlexSans-Medium,IBMPlexSans-SemiBold}.ttf`, `apps/mobile/assets/fonts/OFL.txt`
- Create: `apps/mobile/src/font-assets.ts`, `apps/mobile/__mocks__/expo-font.js`, `apps/mobile/__mocks__/file.js`
- Modify: `apps/mobile/package.json`, `pnpm-lock.yaml`, `apps/mobile/jest.config.js`, `apps/mobile/app/_layout.tsx`
- Test: `apps/mobile/__tests__/font-gate.test.tsx` (create)

**Interfaces:**
- Consumes: `fonts`, `colors` from Task 1.
- Produces: `fontAssets: Record<(typeof fonts)[keyof typeof fonts], number>` in `src/font-assets.ts` (keys are the PostScript names, values `require("../assets/fonts/<file>.ttf")`).
- Produces: `FontGate({ children }: { children: ReactNode })` exported from `app/_layout.tsx` (or `src/font-gate.tsx` if the router rejects non-default exports from `app/`): renders `<View testID="font-gate" style={{ flex: 1, backgroundColor: colors.background }} />` while `useFonts` is `[false, null]`, children once loaded or errored.

- [ ] **Step 1: Fetch the fonts** into `apps/mobile/assets/fonts/`:

```bash
cd apps/mobile/assets/fonts
curl -fsSLO https://raw.githubusercontent.com/google/fonts/main/ofl/anton/Anton-Regular.ttf
curl -fsSLO https://cdn.jsdelivr.net/npm/dseg@0.46.0/fonts/DSEG14-Classic-MINI/DSEG14ClassicMini-Regular.ttf
for w in Regular Medium; do curl -fsSLO https://cdn.jsdelivr.net/npm/@ibm/plex-mono@1.1.0/fonts/complete/ttf/IBMPlexMono-$w.ttf; done
for w in Regular Medium SemiBold; do curl -fsSLO https://cdn.jsdelivr.net/npm/@ibm/plex-sans@1.1.0/fonts/complete/ttf/IBMPlexSans-$w.ttf; done
cp ../../../desktop/src/assets/fonts/OFL.txt .
```

Expected: seven `.ttf` files, each over 10 KB.

- [ ] **Step 2: Verify PostScript names** (name ID 6) match `fonts` in `theme.ts`:

```bash
python3 -I -c 'import struct,sys
for p in sys.argv[1:]:
    d=open(p,"rb").read(); n=struct.unpack(">H",d[4:6])[0]
    t={d[12+16*i:16+16*i]:struct.unpack(">LL",d[20+16*i:28+16*i]) for i in range(n)}[b"name"]
    o=t[0]; c,so=struct.unpack(">HH",d[o+2:o+6])[0:2]
    for i in range(c):
        pid,eid,lid,nid,ln,off=struct.unpack(">6H",d[o+6+12*i:o+18+12*i])
        if nid==6: print(p, d[o+so+off:o+so+off+ln].decode("utf-16-be" if pid in (0,3) else "latin-1")); break' apps/mobile/assets/fonts/*.ttf
```

Expected: each file prints the matching name from `fonts`. If one differs, change that value in `fonts` (Task 1's test then needs no change, since it reads `fonts`).

- [ ] **Step 3: Add the dependency**

Run: `pnpm --filter mobile add expo-font@~57.0.4`
Expected: `apps/mobile/package.json` lists `"expo-font": "~57.0.4"`; lockfile updated.

- [ ] **Step 4: Jest wiring.** `__mocks__/expo-font.js` exports `useFonts = jest.fn(() => [true, null])`. `__mocks__/file.js` exports `1`. In `jest.config.js` `moduleNameMapper` add `"^expo-font$": "<rootDir>/__mocks__/expo-font.js"` and `"\\.(ttf|otf)$": "<rootDir>/__mocks__/file.js"`.

- [ ] **Step 5: Write the failing test** `font-gate.test.tsx`:

```tsx
import { useFonts } from "expo-font";
import { Text } from "react-native";
import ReactTestRenderer, { act } from "react-test-renderer";
import { FontGate } from "../app/_layout"; // or "@/font-gate", per the Interfaces note

const render = async () => {
  let r!: ReactTestRenderer.ReactTestRenderer;
  await act(async () => { r = ReactTestRenderer.create(<FontGate><Text testID="child">x</Text></FontGate>); });
  return r;
};

test("holds a well-colored view while fonts load", async () => {
  jest.mocked(useFonts).mockReturnValueOnce([false, null]);
  const r = await render();
  expect(r.root.findAllByProps({ testID: "child" })).toHaveLength(0);
  expect(r.root.findByProps({ testID: "font-gate" }).props.style).toMatchObject({ backgroundColor: "#0a0908" });
});

test("renders once fonts load", async () => {
  expect((await render()).root.findAllByProps({ testID: "child" }).length).toBeGreaterThan(0);
});

test("still renders, in system fonts, when loading fails", async () => {
  jest.mocked(useFonts).mockReturnValueOnce([false, new Error("bad font")]);
  expect((await render()).root.findAllByProps({ testID: "child" }).length).toBeGreaterThan(0);
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `pnpm --filter mobile exec jest __tests__/font-gate.test.tsx`
Expected: FAIL (`FontGate` is not exported).

- [ ] **Step 7: Implement** `src/font-assets.ts` and `FontGate` (calls `useFonts(fontAssets)`), and wrap `RootLayout`'s tree in it inside `SafeAreaProvider`.

- [ ] **Step 8: Run the mobile suite and typecheck**

Run: `pnpm --filter mobile test && pnpm --filter mobile typecheck`
Expected: PASS, including `App.test.tsx` (the mock reports fonts ready).

- [ ] **Step 9: Commit**

```bash
git add apps/mobile/assets/fonts apps/mobile/src/font-assets.ts apps/mobile/__mocks__/expo-font.js apps/mobile/__mocks__/file.js apps/mobile/jest.config.js apps/mobile/package.json pnpm-lock.yaml apps/mobile/app/_layout.tsx apps/mobile/__tests__/font-gate.test.tsx
git commit -m "feat(mobile): load the desktop's retro fonts at launch"
```

### Task 3: Sign, LED readout and faceplate primitives

**Files:**
- Create: `apps/mobile/src/ui/sign.tsx`, `apps/mobile/src/ui/led-readout.tsx`
- Modify: `apps/mobile/src/ui/card.tsx`, `apps/mobile/src/ui/notice.tsx`, `apps/mobile/src/ui/icon-tile.tsx`
- Test: `apps/mobile/__tests__/retro-primitives.test.tsx` (create)

**Interfaces:**
- Consumes: `colors`, `type`, `radius` from Task 1.
- Produces: `Sign({ lit, testID }: { lit: boolean; testID?: string })` — `accessibilityRole="header"`, `accessibilityLabel="on-air"`, visible text `ONAIR` in `type.sign`; lit: `signLitText` on `signLitWell`, border `signLitLine`, red `textShadow` (radius 8); unlit: `signOffText` on `signOffWell`, border `separator`, no shadow; `maxFontSizeMultiplier={1.3}`.
- Produces: `LedReadout({ value, digits, size, dim, accessibilityLabel, testID }: { value: string; digits: number; size: number; dim?: boolean; accessibilityLabel: string; testID?: string })` — `ledWell` recess, border `separator`, radius `sm`; a ghost layer `"8".repeat(digits)` in `ledGhost`, hidden from accessibility; the value right-aligned over it in `amber` (`tertiaryLabel` when `dim`), `type.led` at `size`, `maxFontSizeMultiplier={1.3}`, amber `textShadow` only when not dim.
- `Card`: radius `lg`, `borderTopColor` one step lighter (`borderStrong`) for the sheen. `Notice`: radius `md`. `IconTile`: radius `md`, sizes unchanged.

- [ ] **Step 1: Write the failing tests**:

```tsx
test("the sign is named on-air and lights only when live", () => {
  const lit = create(<Sign lit testID="s" />).root.findByProps({ testID: "s" });
  expect(lit.props).toMatchObject({ accessibilityRole: "header", accessibilityLabel: "on-air" });
  expect(textStyle(lit)).toMatchObject({ color: "#ff3b2a", fontFamily: "Anton-Regular" });
  expect(textStyle(create(<Sign lit={false} testID="s" />).root.findByProps({ testID: "s" })))
    .toMatchObject({ color: "#5a1814", textShadowRadius: 0 });
});

test("the sign and LED digits cap text scaling", () => {
  expect(allText(create(<Sign lit />)).every((t) => t.props.maxFontSizeMultiplier === 1.3)).toBe(true);
  expect(allText(create(<LedReadout value="70" digits={3} size={20} accessibilityLabel="Volume 70" />))
    .every((t) => t.props.maxFontSizeMultiplier === 1.3)).toBe(true);
});

test("LED readout: ghost segments are hidden, value is amber, dim is muted", () => {
  const r = create(<LedReadout value="70" digits={3} size={20} accessibilityLabel="Volume 70" testID="led" />);
  expect(r.root.findByProps({ testID: "led" }).props.accessibilityLabel).toBe("Volume 70");
  const ghost = r.root.findByProps({ children: "888" });
  expect(ghost.props).toMatchObject({ accessibilityElementsHidden: true, importantForAccessibility: "no" });
  expect(flat(r.root.findByProps({ children: "70" }).props.style)).toMatchObject({ color: "#e0a24b" });
  const dim = create(<LedReadout value="70" digits={3} size={20} dim accessibilityLabel="Volume 70" />);
  expect(flat(dim.root.findByProps({ children: "70" }).props.style)).toMatchObject({ color: "#9a8f78" });
});
```

(`create` wraps `ReactTestRenderer.create`; `flat` is `StyleSheet.flatten`; `textStyle(node)` flattens the style of the first `Text` under `node`; `allText` returns every `Text` instance.)

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter mobile exec jest __tests__/retro-primitives.test.tsx`
Expected: FAIL (modules not found).

- [ ] **Step 3: Implement** `Sign`, `LedReadout` and the `Card` / `Notice` / `IconTile` restyle per the Interfaces block.

- [ ] **Step 4: Run the mobile suite**

Run: `pnpm --filter mobile test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/mobile/src/ui/sign.tsx apps/mobile/src/ui/led-readout.tsx apps/mobile/src/ui/card.tsx apps/mobile/src/ui/notice.tsx apps/mobile/src/ui/icon-tile.tsx apps/mobile/__tests__/retro-primitives.test.tsx
git commit -m "feat(mobile): ONAIR sign, LED readout and faceplate primitives"
```

### Task 4: Native surfaces: hosts, forms, sheets and the navigation theme

**Files:**
- Create: `apps/mobile/src/ui/form-text.tsx`, `apps/mobile/src/navigation-theme.ts`
- Modify: `apps/mobile/src/settings-form.tsx`, `apps/mobile/src/sheets/sheet-props.ts`, `apps/mobile/src/ui/native-icon.tsx`, `apps/mobile/src/ui/firewall-notice.tsx`, `apps/mobile/src/native-fader.tsx`, `apps/mobile/app/_layout.tsx`, `apps/mobile/app/index.tsx`, `apps/mobile/app/sound.tsx`, `apps/mobile/app/equalizer.tsx`, `apps/mobile/app/audio-format.tsx`, `apps/mobile/app/connection.tsx`, `apps/mobile/__mocks__/swift-ui-modifiers.js`, `apps/mobile/__mocks__/navigation-theme.js`
- Test: `apps/mobile/__tests__/native-surfaces.test.tsx` (create)

**Interfaces:**
- Consumes: `colors`, `fonts` (Task 1).
- Produces: `navigationTheme` in `src/navigation-theme.ts` — `{ ...DarkTheme, colors: { ...DarkTheme.colors, background: colors.background, card: colors.surface, text: colors.label, border: colors.separator, primary: colors.amber, notification: colors.accent } }`; `screenHeaderOptions = { headerTintColor: colors.amber, headerTitleStyle: { fontFamily: fonts.monoMedium, color: colors.label } }` spread into the `Stack` `screenOptions` in `_layout.tsx`.
- Produces: `FormText({ children, variant }: { children: string; variant: "body" | "label" | "value" })` — Expo UI `Text`; iOS `modifiers: [font({ family, textStyle })]` with `body → fonts.sans/"body"`, `label → fonts.monoMedium/"footnote"`, `value → fonts.monoMedium/"body"`; `textStyle.color` `label` / `tertiaryLabel` / `amber`; `label` variant uppercases its string.
- Produces: `SectionTitle({ title }: { title: string })` in `form-text.tsx` — `<FieldGroup.SectionHeader><FormText variant="label">{title}</FormText></FieldGroup.SectionHeader>`.
- `SettingsForm`: `Host seedColor={colors.amber}`; on iOS `FieldGroup modifiers={[scrollContentBackground("hidden"), background(colors.background)]}`. Every `FieldGroup.Section title=` in the app screens becomes a `SectionTitle` child, and every section gets `modifiers={[listRowBackground(colors.surface)]}` on iOS. Plain `Text` in rows becomes `FormText` (`body`); the Sound screen trailing values and EQ values use `value`.
- `sheetProps` iOS modifiers: `[environment colorScheme dark, presentationBackground(colors.surface), tint(colors.amber)]`.
- Every `Host` in `native-icon.tsx`, `firewall-notice.tsx`, `settings-form.tsx`, `app/index.tsx` and the mixer volume host (Task 5) passes `seedColor={colors.amber}`.
- `NativeFader` header value text: `textStyle.color = colors.amber`, `fontFamily = fonts.monoMedium`.
- Mocks: add `scrollContentBackground`, `listRowBackground`, `background`, `font` to `swift-ui-modifiers.js` in the existing `modifier($type, params)` shape; the navigation mock's `DarkTheme` gains `colors: {}`.

- [ ] **Step 1: Write the failing tests**:

```tsx
test("navigation theme uses the faceplate palette with amber controls", () => {
  expect(navigationTheme.colors).toMatchObject({ background: "#0a0908", card: "#1a1814", text: "#f3ead4", border: "#2e2a24", primary: "#e0a24b" });
  expect(screenHeaderOptions.headerTintColor).toBe("#e0a24b");
});

test("sheets are faceplate-backed and amber-tinted on iOS", () => {
  expect(sheetProps({ kind: "source" }).modifiers).toEqual(
    expect.arrayContaining([{ $type: "presentationBackground", color: "#1a1814" }, { $type: "tint", color: "#e0a24b" }]),
  );
});

test("settings forms hide the system background and seed amber", () => {
  const host = renderWithSession(<SettingsForm testID="f"><></></SettingsForm>).root.findByType("ExpoHost");
  expect(host.props.seedColor).toBe("#e0a24b");
  expect(host.findByType("ExpoFieldGroup").props.modifiers).toEqual(
    expect.arrayContaining([{ $type: "scrollContentBackground", visible: "hidden" }, { $type: "background", color: "#0a0908" }]),
  );
});

test("form text uses the vendored families and uppercases labels", () => {
  const label = create(<FormText variant="label">Tone</FormText>).root.findByType("ExpoText");
  expect(label.props.children).toBe("TONE");
  expect(label.props.modifiers).toEqual([{ $type: "font", family: "IBMPlexMono-Medium", textStyle: "footnote" }]);
});

test("decorative icons seed amber so Android never falls back to Material purple", () => {
  expect(create(<NativeIcon name="speaker" />).root.findByType("ExpoHost").props.seedColor).toBe("#e0a24b");
});
```

(`renderWithSession` renders with `@/remote-session` mocked via `jest.mock` so `useRemoteSession` returns `{ error: null, warning: null, view: { paused: false } }`. `ExpoText`, `ExpoHost` and `ExpoFieldGroup` are the element names `__mocks__/expo-ui.js` already uses.)

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter mobile exec jest __tests__/native-surfaces.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement** per the Interfaces block, iOS-only modifiers behind `Platform.OS === "ios"` like `sheet-props.ts` does today.

- [ ] **Step 4: Run the mobile suite and typecheck**

Run: `pnpm --filter mobile test && pnpm --filter mobile typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/mobile/src/ui/form-text.tsx apps/mobile/src/navigation-theme.ts apps/mobile/src/settings-form.tsx apps/mobile/src/sheets/sheet-props.ts apps/mobile/src/ui/native-icon.tsx apps/mobile/src/ui/firewall-notice.tsx apps/mobile/src/native-fader.tsx apps/mobile/app apps/mobile/__mocks__/swift-ui-modifiers.js apps/mobile/__mocks__/navigation-theme.js apps/mobile/__tests__/native-surfaces.test.tsx
git commit -m "feat(mobile): amber-tinted native forms, sheets and headers"
```

### Task 5: Mixer home in the retro skin

**Files:**
- Modify: `apps/mobile/src/mixer-home.tsx`
- Test: `apps/mobile/__tests__/App.test.tsx` (add tests)

**Interfaces:**
- Consumes: `Sign`, `LedReadout`, `Card` (Task 3); `colors`, `type` (Task 1).
- `Header`: `Sign lit={status.live} testID="mixer-sign"` replaces the "on-air" `Text`; the status pill keeps `testID="status-pill"` and its `accessibilityLabel`, its text in `type.label` (`accent` when live, `tertiaryLabel` otherwise), its dot `accent` / `liveDim`; the more button is 36 × 36, radius 18, `hitSlop={5}` (46 pt target).
- Section labels ("Now streaming", "Playback") in `type.label`, `tertiaryLabel`.
- `RouteRow`: title `type.title`, subtitle `type.body` in `secondaryLabel`; the output row's title gets a trailing red `●` (decorative, hidden from accessibility) while `status.live`. `RouteDivider` arrow color `amber`.
- Live state lamp color `accent` when live, `liveDim` otherwise; texts `type.title` / `type.body`.
- Volume card: header row label "Volume" in `type.label` and `LedReadout value={String(Math.round(volume))} digits={3} size={20} dim={volumeDisabled} accessibilityLabel={\`Volume ${Math.round(volume)}\`} testID="volume-readout"`; volume `Host seedColor={colors.amber}`.
- `CdDeck`: `testID="cd-track"` stays on a `LedReadout` (value `"01/12"` style string, `digits` = string length, size 16).
- Sound settings row text `type.body` at 17 pt; Disconnect text `type.label` in `onError`.

- [ ] **Step 1: Write the failing tests** in `App.test.tsx`, reusing its existing helpers (`host`, `text`, the paired-and-live setup from the happy-path test):

```tsx
test("retro header: the sign lights while live and stays unlit when ready", async () => {
  // paired, ready (not live)
  expect(host("mixer-sign").props.accessibilityLabel).toBe("on-air");
  expect(signColor("mixer-sign")).toBe("#5a1814");
  // activate Mock Sonos as the happy path does
  expect(signColor("mixer-sign")).toBe("#ff3b2a");
});

test("volume shows on an LED readout that dims when the desktop is paused", async () => {
  expect(host("volume-readout").props.accessibilityLabel).toBe("Volume 50");
  // paused desktop, as in the existing paused test
  expect(ledColor("volume-readout")).toBe("#9a8f78");
});
```

(`Sign` gets `testID="mixer-sign"`; `signColor(id)` / `ledColor(id)` flatten the style of the first / value `Text` under that node.)

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter mobile exec jest __tests__/App.test.tsx -t "retro header|LED readout"`
Expected: FAIL.

- [ ] **Step 3: Implement** the Interfaces block in `mixer-home.tsx`, replacing inline `fontSize` / `fontWeight` with `type` presets throughout the file.

- [ ] **Step 4: Run the mobile suite**

Run: `pnpm --filter mobile test`
Expected: PASS, including the existing `cd-track` and status-pill assertions.

- [ ] **Step 5: Commit**

```bash
git add apps/mobile/src/mixer-home.tsx apps/mobile/__tests__/App.test.tsx
git commit -m "feat(mobile): retro mixer home with ONAIR sign and LED volume"
```

### Task 6: Pairing screen in the retro skin

**Files:**
- Modify: `apps/mobile/src/pairing-home.tsx`
- Test: `apps/mobile/__tests__/App.test.tsx` (add tests)

**Interfaces:**
- Consumes: `Sign`, `Card` (Task 3); `colors`, `type`, `fonts` (Task 1).
- Title: `Sign lit={false} testID="remote-title"` replaces the "on-air" `Text`.
- Eyebrow "Pair your Mac": `type.label`, `amber` (copy unchanged; uppercase via `textTransform`).
- Headline `type.title` at 26 pt; body `type.body` in `secondaryLabel`.
- `DiscoveredRow` selected: `surfaceRaised` background, `key` border, check color `amber`; unselected `tertiaryLabel`. `LinkButton` primary color `amber`. `ActivityIndicator` color `amber`.
- PIN `TextInput` (`testID="pin-input"`): keeps `keyboardType`, `maxLength={6}`, `onSubmitEditing`, `accessibilityLabel`; `placeholder="888888"`, `placeholderTextColor={colors.ledGhost}`, `fontFamily: fonts.led`, `color: colors.amber`, `backgroundColor: colors.ledWell`, border `key` when 6 digits else `separator`, `maxFontSizeMultiplier={1.3}`.
- Connect (`testID="pair-button"`): enabled `amber` background with `onAmber` text; disabled `surface` background with `tertiaryLabel` text; text `type.label` at 13 pt. Host-address `TextInput` keeps its `192.168.1.20` placeholder, styled `type.body`.

- [ ] **Step 1: Write the failing tests**:

```tsx
test("pairing: unlit sign, LED code field and an amber Connect key", async () => {
  // fresh launch with a discovered desktop, as in "pairs with a discovered desktop on its advertised port"
  expect(host("remote-title").props.accessibilityLabel).toBe("on-air");
  const pin = host("pin-input");
  expect(pin.props).toMatchObject({ placeholder: "888888", maxLength: 6, accessibilityLabel: "Six-digit pairing code" });
  expect(flat(host("pair-button").props.style)).toMatchObject({ backgroundColor: "#1a1814" }); // disabled
  // type 123456 into pin-input
  expect(flat(host("pair-button").props.style)).toMatchObject({ backgroundColor: "#e0a24b" });
});
```

(If `pair-button`'s `style` is a function of `pressed`, call it with `{ pressed: false }` before flattening.)

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter mobile exec jest __tests__/App.test.tsx -t "pairing: unlit sign"`
Expected: FAIL.

- [ ] **Step 3: Implement** the Interfaces block in `pairing-home.tsx`, replacing inline type with `type` presets.

- [ ] **Step 4: Run every mobile check**

Run: `pnpm --filter mobile test && pnpm --filter mobile typecheck && pnpm check:ci`
Expected: PASS, including the existing "Pair your Mac" and pairing-flow tests.

- [ ] **Step 5: Commit**

```bash
git add apps/mobile/src/pairing-home.tsx apps/mobile/__tests__/App.test.tsx
git commit -m "feat(mobile): retro pairing screen with LED code entry"
```

### Task 7: Device verification

**Files:** none changed unless a defect is found (fix it in the owning task's files and commit as `fix(mobile): …`).

- [ ] **Step 1: Full check suite**

Run: `pnpm --filter mobile test && pnpm --filter mobile typecheck && pnpm check:ci && pnpm test:mobile`
Expected: all PASS.

- [ ] **Step 2: Get an iOS 27-capable build.** The checked-in `ios/` traps at launch on iOS 27 without the UIScene fix on `refs/heads/shay/interesting-poitras-096ac1`. Ask the user whether to merge that branch here; do not merge without their answer. Without it, verify on Android only and say so.

- [ ] **Step 3: Run against the mock core** (`ON_AIR_MOCK=1 PORT=47990 cargo run -p on-air-core --bin on-air-core`, PIN 123456) and capture: pairing (empty and 6 digits), home idle and live, CD deck (`simulateCd` from `@on-air/control-client`), Sound, Equalizer, Audio format, Connection, source and speaker sheets, and the home screen at the largest accessibility text size.
Expected: palette, fonts and sign as in the spec; no system-gray form backgrounds, no white headers, no purple Material controls; sign and LED fit at the largest text size.

- [ ] **Step 4: Android emulator** (if `emulator -list-avds` lists one): same captures for home, pairing and Equalizer.

- [ ] **Step 5: Report** the captures to the user; note anything not verified.
