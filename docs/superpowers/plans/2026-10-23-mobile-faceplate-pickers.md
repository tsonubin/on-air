# Faceplate Pickers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the source, speaker and speaker-pairing sheets in the mixer's faceplate language (LED window, channel strips, panel keys), with no change to what they do.

**Architecture:** Keep Expo UI's native `BottomSheet` for presentation, with `contentPadding={0}` and a faceplate `containerColor`. Fill it with React Native components. New shared pieces (`Lamp`, `ChannelStrip`, `LedCodeInput`) live in `src/ui/`. A `SheetPanel` wrapper gives the three sheets one header and surface. Sheet logic keeps calling the same `useRemoteSession()` methods.

**Tech Stack:** Expo SDK 58, React Native 0.88, `@expo/ui` 58 (`BottomSheet`), Jest 30 with `react-test-renderer` and the repo's mocks in `apps/mobile/__mocks__/`.

**Spec:** `docs/superpowers/specs/2026-10-23-mobile-faceplate-pickers-design.md`

## Global Constraints

- **No dependencies or core changes.** No new dependencies; no change to `packages/`, the core, or any `useRemoteSession()` method.
- **Red is only for live.** `colors.accent` is used only for live audio (the live lamp, the LIVE caption, the live strip).
- **Text scaling:** names, captions and action words set `maxFontSizeMultiplier={1.3}`. Names wrap to two lines; nothing else truncates.
- **Copy, verbatim from the spec:**
  - Captions: `SOURCE`, `SPEAKER`, `PAIR SPEAKER` (written in Title case; `type.label` uppercases them), and `Capture device`.
  - Source: "Selected", "Select", "Switching…", "No capture devices are available.", "The Mac captures this device and streams it to the speaker."
  - Speaker: "Waiting for speakers on the LAN…", "AirPlay speakers are picked from the desktop.", "+ Add Bluetooth speaker".
  - Pairing: "Enter the code the speaker shows. No code? Leave it blank.", "Confirm pairing on the speaker or in the computer's Bluetooth settings, then continue.", "Cancel", "Pair and connect", "Pairing…".
- **Kept testIDs:** `source-sheet`, `output-sheet`, `pair-sheet`, `add-bluetooth`, `output-<transport>-<id>`, `device-pin-input`, `device-pair-submit`, `device-pair-cancel`, `pin-input`, `live-lamp`.
- **New testIDs:**
  - `input-<name>`, and `<sheet testID>-done`.
  - `source-led`, `speaker-led`, `speaker-led-state`.
  - `<strip testID>-lamp`, `<strip testID>-action`.
- **Gates** (from `apps/mobile`): `pnpm test`, `pnpm run typecheck`, `pnpm exec biome check src __tests__ app`. All pass before each commit; Biome's one existing warning in `src/hooks/useDesktopConnection.ts` is on `main` and stays.
- **Commits:** stage explicit paths only, no attribution lines, never force-push. Branch `feat/mobile-faceplate-pickers`.
- **Metro:** run it without `CI=1` (CI mode has no watcher and serves stale bundles).

## Review Focus

1. **A long name** ("Living Room Sonos Arc Ultra with Sub Mini") wraps to two lines, and the action word stays fully visible on the right. Test in Task 1.
2. **A second tap while a pick is running** does nothing: every strip is disabled while `busyTarget` is set. Test in Tasks 4 and 5.
3. **The connection drops with a sheet open** (`controlsEnabled` false): strips disable, and Done still closes the sheet. Test in Tasks 4 and 5.
4. **The active speaker is missing from the list** (it dropped off discovery): the LED window still shows `activeOutput.device_name`. Test in Task 5.
5. **Swipe-down or Android back** closes the sheet through `onDismiss`, so sheet state never sticks open. Test in Task 3.

---

### Task 1: `Lamp` and `ChannelStrip`

**Files:**
- Create: `apps/mobile/src/ui/lamp.tsx`, `apps/mobile/src/ui/channel-strip.tsx`
- Modify: `apps/mobile/src/led-display.tsx` (the inline `live-lamp` view becomes `Lamp`)
- Test: `apps/mobile/__tests__/channel-strip.test.tsx` (new)

**Interfaces:**
- Produces:
  - `type LampState = "off" | "amber" | "live" | "error"`
  - `Lamp({ state, size = 7, testID }: { state: LampState; size?: number; testID?: string })`
  - `type StripTone = "idle" | "selected" | "connecting" | "live" | "failed" | "unavailable"`
  - `ChannelStrip({ name, caption, action, tone, disabled, onPress, accessibilityHint, testID }: { name: string; caption: string; action: string; tone: StripTone; disabled?: boolean; onPress: () => void; accessibilityHint?: string; testID: string })`

- [ ] **Step 1: Write the failing tests** in `__tests__/channel-strip.test.tsx`. Use the `create`, `hostById`, `flat` and `textWith` helpers, copied from `retro-primitives.test.tsx`.

```tsx
test("lamps glow only when amber or live", () => {
  const style = (state: LampState) => flat(hostById(create(<Lamp state={state} testID="l" />), "l").props.style);
  expect(style("off")).toMatchObject({ width: 7, height: 7, borderRadius: 3.5, backgroundColor: "#3a332a", shadowOpacity: 0 });
  expect(style("amber")).toMatchObject({ backgroundColor: "#e0a24b", shadowColor: "#e0a24b", shadowOpacity: 0.6 });
  expect(style("live")).toMatchObject({ backgroundColor: "#ff3b2a", shadowColor: "#ff3b2a", shadowOpacity: 0.6 });
  expect(style("error")).toMatchObject({ backgroundColor: "#f0b4ac", shadowOpacity: 0 });
});

test.each([
  ["idle", "#3a332a", "rgba(42,38,31,0.55)", "#2e2a24", "#e0a24b"],
  ["selected", "#e0a24b", "rgba(42,38,31,0.55)", "#5c4a32", "#f0c56a"],
  ["connecting", "#e0a24b", "rgba(42,38,31,0.55)", "#2e2a24", "#e0a24b"],
  ["live", "#ff3b2a", "#3a221c", "#8a3a30", "#ff3b2a"],
  ["failed", "#f0b4ac", "#3a1410", "#6a2a22", "#f0b4ac"],
  ["unavailable", "#3a332a", "rgba(42,38,31,0.55)", "#2e2a24", "#9a8f78"],
] as const)("a %s strip", (tone, lamp, surface, border, ink) => {
  const r = create(<ChannelStrip name="Den" caption="Sonos" action="Go" tone={tone} onPress={jest.fn()} testID="s" />);
  expect(flat(hostById(r, "s-lamp").props.style).backgroundColor).toBe(lamp);
  expect(flat(hostById(r, "s").props.style)).toMatchObject({ backgroundColor: surface, borderColor: border, minHeight: 44 });
  expect(flat(hostById(r, "s-action").props.style).color).toBe(ink);
  expect(flat(hostById(r, "s").props.style).opacity).toBe(tone === "unavailable" ? 0.45 : undefined);
});

test("a strip reads as one button with its state", () => {
  const r = create(<ChannelStrip name="Den" caption="Sonos" action="Live" tone="live" onPress={jest.fn()} testID="s" />);
  expect(hostById(r, "s").props).toMatchObject({
    accessibilityRole: "button",
    accessibilityLabel: "Den, Sonos",
    accessibilityValue: { text: "Live" },
    accessibilityState: { selected: true, busy: false, disabled: false },
  });
  const busy = create(<ChannelStrip name="Den" caption="Sonos" action="Connecting…" tone="connecting" onPress={jest.fn()} testID="s" />);
  expect(hostById(busy, "s").props.accessibilityState).toMatchObject({ selected: false, busy: true });
});

test("a disabled strip ignores presses", () => {
  const r = create(<ChannelStrip name="Den" caption="Sonos" action="Connect" tone="idle" disabled onPress={jest.fn()} testID="s" />);
  expect(hostById(r, "s").props.onPress).toBeUndefined();
  expect(hostById(r, "s").props.accessibilityState).toMatchObject({ disabled: true });
});

test("a long name wraps beside a whole action word", () => {
  const r = create(<ChannelStrip name="Living Room Sonos Arc Ultra with Sub Mini" caption="Sonos" action="Failed · Retry" tone="failed" onPress={jest.fn()} testID="s" />);
  const name = textWith(r, "Living Room Sonos Arc Ultra with Sub Mini");
  expect(name.props).toMatchObject({ numberOfLines: 2, maxFontSizeMultiplier: 1.3 });
  expect(flat(hostById(r, "s-text").props.style).flex).toBe(1);
  const action = hostById(r, "s-action");
  expect(action.props).toMatchObject({ numberOfLines: 1, maxFontSizeMultiplier: 1.3 });
  expect(flat(action.props.style).flexShrink).toBe(0);
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `pnpm exec jest __tests__/channel-strip.test.tsx`
Expected: FAIL, "Cannot find module '@/ui/lamp'".

- [ ] **Step 3: Implement `Lamp` in `src/ui/lamp.tsx`.**
  - A `View` with these colours: off `liveDim`, amber `amber`, live `accent`, error `onError`.
  - `shadowColor` is the lamp's own colour, with `shadowOffset` 0/0 and `shadowRadius` 6. `shadowOpacity` is 0.6 for amber and live, and 0 otherwise.
  - Hidden from accessibility (`accessibilityElementsHidden`, `importantForAccessibility="no-hide-descendants"`).

- [ ] **Step 4: Implement `ChannelStrip` in `src/ui/channel-strip.tsx`.**
  - A row `Pressable`: `<Lamp testID={`${testID}-lamp`}>`, then a text column (`flex: 1`, `testID={`${testID}-text`}`) holding the name and caption, then the action.
  - Pressed state: `surfacePressed` replaces the surface, as `PanelKey` darkens.
  - Name: `fonts.sansMedium`, 15/20, `colors.label`, `numberOfLines={2}`.
  - Caption: `type.label`, size 10, `tertiaryLabel`.
  - Action: `type.label` in the tone's ink, `numberOfLines={1}`, `flexShrink: 0`, `testID={`${testID}-action`}`.
  - Surface: border width 1, `radius.lg` (8), padding 10/12, gap 10.
  - Tone to lamp: idle off, selected amber, connecting amber, live live, failed error, unavailable off.
  - `accessibilityState` is `{ selected: tone === "selected" || tone === "live", busy: tone === "connecting", disabled: !!disabled }`.

- [ ] **Step 5: Swap the inline lamp in `src/led-display.tsx`** for `<Lamp state={status.live ? "live" : "off"} size={8} testID="live-lamp" />`.

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `pnpm exec jest __tests__/channel-strip.test.tsx __tests__/led-display.test.tsx __tests__/App.test.tsx`
Expected: PASS. The `live-lamp` assertions in `led-display.test.tsx` and `App.test.tsx` are unchanged.

- [ ] **Step 7: Commit** `feat(mobile): a lamp and a channel strip for the faceplate pickers`, staging the four files.

---

### Task 2: `LedCodeInput`

**Files:**
- Create: `apps/mobile/src/ui/led-code-input.tsx`
- Modify: `apps/mobile/src/pairing-home.tsx` (the `pin-input` `TextInput` becomes `LedCodeInput`)
- Test: `apps/mobile/__tests__/retro-primitives.test.tsx`

**Interfaces:**
- Produces: `LedCodeInput(props: Omit<TextInputProps, "style"> & { length: number; compact?: boolean; ref?: Ref<TextInputInstance> })`. It is a React 19 ref-as-prop component and forwards every other `TextInput` prop.

- [ ] **Step 1: Write the failing test** in `retro-primitives.test.tsx`:

```tsx
test("the LED code field shows ghost 8s and amber digits in a well", () => {
  const r = create(<LedCodeInput length={8} value="4721" onChangeText={jest.fn()} testID="c" />);
  const field = hostById(r, "c");
  expect(field.props).toMatchObject({
    placeholder: "88888888", placeholderTextColor: "#1c1812", cursorColor: "#e0a24b",
    keyboardType: "number-pad", inputMode: "numeric", maxLength: 8, maxFontSizeMultiplier: 1.3, value: "4721",
  });
  expect(flat(field.props.style)).toMatchObject({
    backgroundColor: "#070605", fontFamily: "DSEG14ClassicMini-Regular", color: "#e0a24b",
    fontSize: 27, letterSpacing: 9, textAlign: "center", minHeight: 66,
  });
  const compact = hostById(create(<LedCodeInput length={6} compact testID="c" />), "c");
  expect(flat(compact.props.style)).toMatchObject({ fontSize: 24, letterSpacing: 6 });
  expect(compact.props.placeholder).toBe("888888");
});
```

- [ ] **Step 2: Run it and confirm it fails.** Run: `pnpm exec jest __tests__/retro-primitives.test.tsx`. Expected: FAIL, module not found.

- [ ] **Step 3: Implement `LedCodeInput`.** Move the `pin-input` `TextInput`'s look out of `pairing-home.tsx`: `wellStyle`, then `minHeight: 66, paddingHorizontal: 14, fontFamily: fonts.led, color: colors.amber`, with `compact` choosing 24/6 and otherwise 27/9, and `textAlign: "center"`. The placeholder is `"8".repeat(length)`. Also `selectionHandleColor` amber and `maxLength={length}`.

- [ ] **Step 4: Use it in `pairing-home.tsx`:** `<LedCodeInput length={6} compact={compact} ref={pinInputRef} … />`. Keep the field's own props (`value`, `onChangeText`, `returnKeyType`, `onSubmitEditing`, `onFocus`, `editable`, `accessibilityLabel`, `testID="pin-input"`).

- [ ] **Step 5: Run the tests and confirm they pass.** Run: `pnpm exec jest __tests__/retro-primitives.test.tsx __tests__/App.test.tsx`. Expected: PASS, with the pairing tests that drive `pin-input` unchanged.

- [ ] **Step 6: Commit** `refactor(mobile): share the LED code field`.

---

### Task 3: `sheetProps` and `SheetPanel`

**Files:**
- Modify: `apps/mobile/src/sheets/sheet-props.ts`
- Create: `apps/mobile/src/sheets/sheet-panel.tsx`
- Test: `apps/mobile/__tests__/pure-helpers.test.ts` (the `sheetProps` block), `apps/mobile/__tests__/native-surfaces.test.tsx` ("sheets are faceplate-backed…"), and `apps/mobile/__tests__/picker-sheets.test.tsx` (new)

**Interfaces:**
- Produces:
  - `type SheetSpec = { kind: "source"; count: number } | { kind: "output"; count: number } | { kind: "device-pair"; transport: string }`
  - `sheetProps(spec)` now also returns `contentPadding: 0`.
  - `SheetPanel({ spec, caption, onClose, done = true, testID, children }: { spec: SheetSpec; caption: string; onClose: () => void; done?: boolean; testID: string; children: ReactNode })`

- [ ] **Step 1: Rewrite the `sheetProps` tests** in `pure-helpers.test.ts`.
  - iOS:
    - `source` count 1 → `[{ height: 280 }]`; count 3 → `[{ height: 358 }]`; count 6 → `["full"]`.
    - `output` count 0 → `[{ height: 300 }]`; count 2 → `[{ height: 332 }]`; count 4 → `[{ height: 464 }]`; count 6 → `["full"]`.
    - `device-pair` keeps 360 for AirPlay and 300 for Bluetooth.
    - `modifiers` is `[{ $type: "environment", key: "colorScheme", value: "dark" }, { $type: "tint", color: "#e0a24b" }]`.
    - `containerColor` is `"#1f1b16"`, and `contentPadding` is `0`.
  - Android, every spec: `{ showDragIndicator: true, containerColor: "#1f1b16", contentPadding: 0 }`.
  - Update `native-surfaces.test.tsx`: `sheetProps({ kind: "source", count: 1 })` has no `presentationBackground` modifier, and has `containerColor: "#1f1b16"`.

- [ ] **Step 2: Write the failing `SheetPanel` tests** in the new `__tests__/picker-sheets.test.tsx`:

```tsx
test("a sheet panel is a full-bleed faceplate with a caption and Done", () => {
  const onClose = jest.fn();
  const r = create(<SheetPanel spec={{ kind: "source", count: 1 }} caption="Source" onClose={onClose} testID="p"><Text>Body</Text></SheetPanel>);
  const sheet = r.root.find((n) => (n.type as unknown) === "ExpoBottomSheet");
  expect(sheet.props).toMatchObject({ isPresented: true, contentPadding: 0, containerColor: "#1f1b16", testID: "p" });
  sheet.props.onDismiss(); // swipe-down / Android back
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(hostById(r, "p-caption").props).toMatchObject({ accessibilityRole: "header", children: "Source" });
  hostById(r, "p-done").props.onPress();
  expect(onClose).toHaveBeenCalledTimes(2);
  expect(r.root.findAll((n) => n.props.testID === "faceplate-finish" && typeof n.type === "string")).toHaveLength(1);
});

test("the pair panel has no Done key", () => {
  const r = create(<SheetPanel spec={{ kind: "device-pair", transport: "airplay" }} caption="Pair speaker" done={false} onClose={jest.fn()} testID="p"><Text>Body</Text></SheetPanel>);
  expect(r.root.findAll((n) => n.props.testID === "p-done")).toHaveLength(0);
});
```

- [ ] **Step 3: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/pure-helpers.test.ts __tests__/native-surfaces.test.tsx __tests__/picker-sheets.test.tsx`. Expected: FAIL.

- [ ] **Step 4: Update `sheet-props.ts`.**
  - Add `"contentPadding"` to the picked `BottomSheetProps` keys.
  - iOS:
    - `source`: `count > 5 ? ["full"] : [{ height: Math.max(280, 160 + count * 66) }]`.
    - `output`: `count > 5 ? ["full"] : [{ height: Math.max(300, 200 + count * 66) }]`.
    - Modifiers: drop `presentationBackground`.
    - Add `containerColor: colors.faceTop` and `contentPadding: 0`.
  - Android: `{ showDragIndicator: true, containerColor: colors.faceTop, contentPadding: 0 }`. Drop `contentColor`, which doesn't reach React Native children.

- [ ] **Step 5: Implement `SheetPanel` in `src/sheets/sheet-panel.tsx`.**
  - Structure: `BottomSheet {...sheetProps(spec)} isPresented onDismiss={onClose} testID={testID}`, containing a `Faceplate` (with `flex: 0, flexGrow: 1` so it fills an iOS detent and sizes to content on Android).
  - Padding: horizontal 16, top 8, bottom `Math.max(insets.bottom, 16)`, gap 12.
  - Header row:
    - The caption `Text`: `type.label`, `tertiaryLabel`, `maxFontSizeMultiplier` 1.3, `accessibilityRole="header"`, `testID={`${testID}-caption`}`.
    - When `done` is set, a `PanelKey label="Done" onPress={onClose} testID={`${testID}-done`}` at its compact size (`paddingHorizontal` 12, `minHeight` 36).
  - Then `children`.

- [ ] **Step 6: Run the tests and confirm they pass.** Use the same command as Step 3. Expected: PASS. `typecheck` now fails in `source-sheet.tsx`, because `source` needs `count`. Pass `{ kind: "source", count: inputs.length }` there now so the gates stay green until Task 4 rewrites it.

- [ ] **Step 7: Commit** `feat(mobile): a faceplate sheet panel`.

---

### Task 4: Source sheet

**Files:**
- Modify: `apps/mobile/src/sheets/source-sheet.tsx` (rewrite)
- Test: `apps/mobile/__tests__/picker-sheets.test.tsx`, `apps/mobile/__tests__/App.test.tsx` (the source pick at lines 268–274)

**Interfaces:**
- Consumes:
  - `SheetPanel` (Task 3), `ChannelStrip` and `Lamp` (Task 1).
  - `LedText` and `LedWindow` (existing).
  - From `useRemoteSession()`: `inputs: { name; label }[]`, `activeInput: string | null`, `activeInputLabel: string`, `busyTarget: string | null`, `view.controlsEnabled`, `pickInput(name): Promise<boolean>`.
- Produces: `SourceSheet({ onClose })`, with an unchanged signature.

- [ ] **Step 1: Write the failing tests** in `picker-sheets.test.tsx`.
  - The session mock:

  ```tsx
  let mockSession: Record<string, unknown>;
  jest.mock("@/remote-session", () => ({ ...jest.requireActual("@/remote-session"), useRemoteSession: () => mockSession }));
  ```

  - A `session(overrides)` factory. Inputs:
    `[{ name: "mock.monitor", label: "Mock Monitor" }, { name: "mic", label: "MacBook Pro Microphone" }]`
    Other fields: `activeInput: "mock.monitor"`, `activeInputLabel: "Mock Monitor"`, `busyTarget: null`, `view: { controlsEnabled: true }`, `pickInput: jest.fn(async () => true)`.
  - The tests:
    - The LED window: `hostById(r, "source-led").props.children` is `"Mock Monitor"`. With `activeInputLabel: ""` it is `"No source"`, dim.
    - The strips:
      - `input-mock.monitor` is selected, with action "Selected" and caption "Capture device".
      - `input-mic` is idle, with action "Select".
    - Pressing `input-mic` calls `pickInput("mic")`, then `onClose` once.
    - When `pickInput` resolves `false`, `onClose` is not called.
    - Pressing the selected strip calls `onClose` and not `pickInput`.
    - With `busyTarget: "input:mic"`, `input-mic` reads "Switching…" with tone connecting, and both strips have `onPress` undefined. (Review Focus 2.)
    - With `view.controlsEnabled: false`, both strips are disabled and `source-sheet-done` still calls `onClose`. (Review Focus 3.)
    - With `inputs: []` and `activeInputLabel: ""`, the text includes "No capture devices are available." Otherwise it includes "The Mac captures this device and streams it to the speaker."
  - In `App.test.tsx`, replace `host("source-picker").props.onValueChange("PipeWire Sound Server")` with `await press("input-PipeWire Sound Server")`. The following assertions stay.

- [ ] **Step 2: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/picker-sheets.test.tsx __tests__/App.test.tsx`. Expected: FAIL.

- [ ] **Step 3: Rewrite `source-sheet.tsx`.**
  - `SheetPanel spec={{ kind: "source", count: inputs.length }} caption="Source" testID="source-sheet"`.
  - The `LedWindow` (row, gap 10, `accessible`, `accessibilityRole="text"`, `accessibilityLabel` of `Source ${label}` or "No source chosen") holds `Lamp` (amber when there's a label) and `<LedText size={18} dim={!label} testID="source-led">{label || "No source"}</LedText>`.
  - Then a `Groove`, then a `ScrollView` of `ChannelStrip`s, then the footnote (`type.body`, 12/16, `tertiaryLabel`, `maxFontSizeMultiplier` 1.3).
  - Strip disabled: `!view.controlsEnabled || busyTarget !== null`.
  - Selected strip press: `onClose()`. Other strips: `if (await pickInput(name)) onClose()`.

- [ ] **Step 4: Run the tests and confirm they pass.** Use the same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): the source picker as faceplate channel strips`.

---

### Task 5: Speaker sheet and `outputTone()`

**Files:**
- Modify: `apps/mobile/src/sheets/output-sheet.tsx` (rewrite the view; keep `outputAction`)
- Test: `apps/mobile/__tests__/pure-helpers.test.ts`, `apps/mobile/__tests__/picker-sheets.test.tsx`

**Interfaces:**
- Consumes:
  - `SheetPanel`, `ChannelStrip`, `Lamp` and `StripTone` (Tasks 1 and 3); `PanelKey`; `LedText` and `LedWindow`.
  - From `useRemoteSession()`: `outputs`, `activeOutput`, `airplayMode`, `busyTarget`, `view.controlsEnabled`, `activate`, `openBluetoothSettings`.
- Produces:
  - `outputTone({ desktopOnly, working, selected, phase }: { desktopOnly: boolean; working: boolean; selected: boolean; phase: OutputPhase | null }): StripTone`
  - `OutputSheet({ onClose, onPairRequest })`, with an unchanged signature.

- [ ] **Step 1: Write the failing tests.**
  - In `pure-helpers.test.ts`, test `outputTone()` against the spec table:

  ```ts
  const off = { desktopOnly: false, working: false, selected: false, phase: null };
  expect(outputTone({ ...off, desktopOnly: true, working: true })).toBe("unavailable");
  expect(outputTone({ ...off, working: true })).toBe("connecting");
  expect(outputTone({ ...off, selected: true, phase: "starting" })).toBe("connecting");
  expect(outputTone({ ...off, selected: true, phase: "failed" })).toBe("failed");
  expect(outputTone({ ...off, selected: true, phase: "live" })).toBe("live");
  expect(outputTone(off)).toBe("idle");
  ```

  - In `picker-sheets.test.tsx`, use the mock core's four outputs:
    - Mock Sonos (sonos); Living Room AirPlay (airplay); Mock Bluetooth Speaker (bluetooth); New Bluetooth Speaker (bluetooth, `needs_pair: true, paired: false`).
    - Plus `airplayMode: "avroute-picker"` and `activeOutput: { transport: "sonos", device_id: "<sonos id>", device_name: "Mock Sonos", state: "live" }`.
  - The tests:
    - The live row: the `output-sonos-…` action is "Connected", its lamp is `#ff3b2a`, and its surface is `#3a221c`.
    - The LED window: `speaker-led` is "Mock Sonos" and `speaker-led-state` is "Live".
    - The AirPlay row is unavailable ("Desktop only", `onPress` undefined), and the text has "AirPlay speakers are picked from the desktop."
    - With `state: "starting"`, the window state reads "Connecting…"; with `"failed"`, the Sonos row's action is "Failed · Retry", tone failed.
    - Pressing New Bluetooth Speaker calls `onPairRequest` with that output and does not call `activate`.
    - Pressing Mock Bluetooth Speaker calls `activate`, then `onClose` once.
    - `add-bluetooth` calls `openBluetoothSettings`.
    - With `busyTarget` set, every strip's `onPress` is undefined. (Review Focus 2.)
    - With `controlsEnabled: false`, the strips and `add-bluetooth` are disabled, and `output-sheet-done` still calls `onClose`. (Review Focus 3.)
    - With `outputs: []` and `activeOutput: null`, `speaker-led` reads "Scanning" and the text has "Waiting for speakers on the LAN…".
    - With `activeOutput` set to a speaker that's not in `outputs`, `speaker-led` still shows its `device_name`. (Review Focus 4.)

- [ ] **Step 2: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/pure-helpers.test.ts __tests__/picker-sheets.test.tsx`. Expected: FAIL.

- [ ] **Step 3: Implement `outputTone`** in `output-sheet.tsx`, next to `outputAction`. Check the conditions in the table's order.

- [ ] **Step 4: Rewrite the `OutputSheet` view.**
  - `SheetPanel spec={{ kind: "output", count: outputs.length }} caption="Speaker" testID="output-sheet"`.
  - The `LedWindow` row:
    - `Lamp`: live, amber or error by phase; off with no active output.
    - `<LedText size={18} dim={!activeOutput} testID="speaker-led">`, reading `activeOutput.device_name`, "Scanning" (no outputs and no active one) or "No speaker".
    - A right-hand `speaker-led-state` caption (`type.label`, `maxFontSizeMultiplier` 1.3): "Live" in `accent`, "Connecting…" in `amber`, or "Failed" in `onError`.
    - `accessibilityLabel` is `Speaker ${name}, live` / `, connecting` / `, failed`, "No speaker chosen", or "Scanning for speakers".
  - Then a `Groove` and a `ScrollView` of strips. Keep today's `selected`, `phase`, `desktopOnly`, `working`, `stereoPair`, the caption, the `choose()` routing and the `testID`. The tone comes from `outputTone`, the action from `outputAction`, and disabled is `!view.controlsEnabled || busyTarget !== null || desktopOnly`.
  - Then `<PanelKey label="+ Add Bluetooth speaker" accessibilityLabel="Add Bluetooth speaker" testID="add-bluetooth" disabled={!view.controlsEnabled} …/>` at full width.
  - Then the footnotes.

- [ ] **Step 5: Run the tests and confirm they pass.** Run: `pnpm exec jest __tests__/pure-helpers.test.ts __tests__/picker-sheets.test.tsx __tests__/App.test.tsx`. Expected: PASS. The App flows that press `output-…` are unchanged.

- [ ] **Step 6: Commit** `feat(mobile): the speaker picker as faceplate channel strips`.

---

### Task 6: Pair speaker sheet

**Files:**
- Modify: `apps/mobile/src/sheets/device-pair-sheet.tsx` (rewrite)
- Test: `apps/mobile/__tests__/picker-sheets.test.tsx`, `apps/mobile/__tests__/App.test.tsx` (the AirPlay pairing test, line 784)

**Interfaces:**
- Consumes:
  - `SheetPanel` (with `done={false}`), `LedCodeInput` (Task 2), `PanelKey` (`lit`).
  - From `remote-session`: `DEVICE_PIN_LENGTH` (8), `digitsOnly`, `useRemoteSession().devicePairing`, `pairDevice(target, pin)`.
- Produces: `DevicePairSheet({ target, onClose })`, with an unchanged signature.

- [ ] **Step 1: Write the failing tests** in `picker-sheets.test.tsx`.
  - AirPlay target `{ transport: "airplay", id: "ap", name: "Living Room AirPlay" }`:
    - The caption is `"Pair speaker"`, and the name text is present.
    - The text has "Enter the code the speaker shows. No code? Leave it blank."
    - `device-pin-input` has `maxLength` 8.
    - `onChangeText("12a34b5678x9")` leaves `value` at `"12345678"`.
    - `device-pair-submit` calls `pairDevice(target, "12345678")`, then `onClose` once it resolves `true`.
    - `device-pair-cancel` calls `onClose`.
  - With `devicePairing: true`: the submit key's label is "Pairing…", and both keys have `onPress` undefined.
  - Bluetooth target: there is no `device-pin-input`, and the text has "Confirm pairing on the speaker or in the computer's Bluetooth settings, then continue."
  - In `App.test.tsx`, replace the `borderColor` assertion with `expect(flat(host("device-pin-input").props.style)).toMatchObject({ color: "#e0a24b", backgroundColor: "#070605" })` (amber digits in an LED well, still not red).

- [ ] **Step 2: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/picker-sheets.test.tsx __tests__/App.test.tsx`. Expected: FAIL.

- [ ] **Step 3: Rewrite `device-pair-sheet.tsx`.**
  - `SheetPanel spec={{ kind: "device-pair", transport }} caption="Pair speaker" done={false} testID="pair-sheet"`.
  - The name: `type.title`, size 18.
  - The footnote: `type.body`, 12/16, `secondaryLabel`.
  - AirPlay only: `<LedCodeInput length={DEVICE_PIN_LENGTH} value={pin} onChangeText={(v) => setPin(digitsOnly(v, DEVICE_PIN_LENGTH))} editable={!devicePairing} accessibilityLabel="Speaker code" testID="device-pin-input" />`, with `pin` in `useState`.
  - A key row: `Cancel` (`flex: 1`) and `lit` `Pair and connect` / `Pairing…` (`flex: 2`), both `disabled={devicePairing}`.
  - Drop the `@expo/ui` imports.

- [ ] **Step 4: Run the tests and confirm they pass.** Run: `pnpm exec jest`. Expected: PASS, with the whole suite green.

- [ ] **Step 5: Commit** `feat(mobile): the speaker pairing sheet on the faceplate`.

---

### Task 7: Device pass

**Files:** none planned. Fixes found here follow Task-style steps, test-first, then a commit each.

- [ ] **Step 1: Run every gate.** From `apps/mobile`, run `pnpm test`, `pnpm run typecheck` and `pnpm exec biome check src __tests__ app`. Expected: all pass.

- [ ] **Step 2: iPhone 17 Pro** (`97B907E2-4E60-46F6-9F83-E562CB5B5C52`), with Metro without `CI=1` and the mock core on `127.0.0.1:47995` (PIN 123456).
  - Open each sheet, and check it against `sheets-a.html`.
  - Pick the source and a speaker, and confirm with `on-air status --json --base http://127.0.0.1:47995`.
  - Open New Bluetooth Speaker's pair sheet. For the code field, use an AirPlay speaker that needs a code if the mock offers one; otherwise check `LedCodeInput` on the first-run screen.
  - Confirm the code field stays above the keyboard. If it doesn't, raise the AirPlay `device-pair` detent (test first), and record a ruling.
- [ ] **Step 3: The largest text size** (`xcrun simctl ui <udid> content_size accessibility-extra-extra-extra-large`, then a cold launch).
  - Names wrap, action words stay whole, and nothing clips. Reset with `content_size large`.
  - If a strip list clips inside its detent, tune the base constant in `sheet-props.ts` and its test, and record a ruling.
- [ ] **Step 4: VoiceOver spot check.** Use the Accessibility Inspector, or the strip's props in a dev log. A strip reads "Mock Sonos, Sonos, Connected, button", and the LED window reads as one element.
- [ ] **Step 5: Folded iPhone Duo** (`1497A209-2326-4F44-9C2B-EA9EA3771150`): the sheets clear the right-hand rail. **Android emulator** (`onair_pixel8_api36`): the sheets draw on the faceplate, and back closes them.
- [ ] **Step 6: Record** the checks and any rulings in the plan ledger, and commit fixes with their tests.
