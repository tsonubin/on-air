# Mobile LED Faceplate Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the phone remote's mixer home and equalizer as one satin LED faceplate, with a recessed display window, a custom volume fader, beveled keys and five vertical EQ faders, while keeping every feature and commit rule.

**Architecture:**
- **Primitives:** new building blocks in `src/ui/`: `Faceplate`, `Groove`, `LedWindow`/`wellStyle`, `LedText`, `PanelKey` and `Fader`. They use React Native 0.86's own gradients, box shadows and `PanResponder`.
- **Commit logic:** `NativeFader`'s logic moves into `useFaderDraft`, so the custom fader commits exactly as the native slider does today.
- **Pure helpers:** `homeStatus`, `formatRate` and `formatDb` drive the LED text.
- **Order:** the screens change one at a time, and the app works after every task.

**Tech Stack:** Expo SDK 57, React Native 0.86.3 (New Architecture), Expo Router, `@expo/ui` 57, Jest 29 with `react-test-renderer` and `@testing-library/react-native`, Biome.

**Spec:** `docs/superpowers/specs/2026-10-22-mobile-led-faceplate-redesign-design.md`

## Global Constraints

- **Dependencies:** none are added, and there are no new native modules, `pod install` or Gradle edits. The exception is Task 0's merged Android fix, which is already written.
- **Colors** come only from `src/theme.ts` tokens, with the values in the spec's token tables.
- **Red** (`accent`) means live only: the lit sign and the lit lamp.
- **Touch targets** are at least `layout.minTarget` (44).
- **LED text and key labels** use `maxFontSizeMultiplier={1.3}`.
- **Surfaces:**
  - Recessed: LED windows, fader slots and text fields only.
  - Raised: keys and fader caps only.
  - No borders around groups.
- **Decorative layers** set `accessibilityElementsHidden` and `importantForAccessibility="no-hide-descendants"` (`"no"` on a single `Text`).
- **Commit timing** stays `theme.motion.controlCommitMs` (180) and `interactionSettleMs` (300).
- **Test IDs** keep their meaning. Only the IDs in the spec's "Existing tests that change only their path" table go away.
- **Commands** run from `apps/mobile` in the worktree, except `pnpm check:ci` and `pnpm test:native`, which run from the repo root (`../..`).
- **Commits** use conventional `feat(mobile):` / `fix(mobile):` / `test(mobile):` / `docs(mobile):` messages with no attribution lines.

## Review Focus

1. **Long or non-ASCII names,** such as `Küche – HomePod 🎵 Wohnzimmer Lautsprecher`, stay on one line in Plex Mono and never wrap or overflow the window. Tested in Task 3 and Task 8.
2. **A desktop value arriving mid-drag** (polling or an event) never moves the cap away from the finger. The latest value is applied after the settle. Tested in Task 5.
3. **A fader disabled mid-drag** (the speaker drops out) ignores further moves and sends no new commit. Tested in Task 5.
4. **A touch before the first layout** (track length 0) is ignored with no NaN. **An out-of-range desktop value** (volume 120) pins the cap at the end. Tested in Task 5.
5. **The largest text size** (font scale 1.6) wraps the four keys into a 2 × 2 grid. Tested in Task 7.

---

### Task 0: Bring in the unmerged mobile fixes (needs the owner's OK)

These branches change files this plan rewrites, so they must be in before Task 1.

**Branches:**
- `refs/heads/shay/elastic-edison-283413`: Android dark-only. It also contains `1c118b8` and `bd74f73`.
- `refs/heads/shay/dazzling-chebyshev-dd737a`: Android EQ Reset, `rowColors`, amber caret.
- `refs/heads/shay/sweet-bell-f8e85a`: keyboard clearance for Connect. It is based on `7773478`, so it conflicts.

- [ ] **Step 1: Merge the two Android branches**

```bash
git merge --no-ff refs/heads/shay/elastic-edison-283413 -m "Merge the Android dark-only fix"
git merge --no-ff refs/heads/shay/dazzling-chebyshev-dd737a -m "Merge the Android Equalizer Reset fix"
```

Expected: both merge cleanly, because they share `1c118b8` and `bd74f73`.

- [ ] **Step 2: Merge the keyboard-clearance branch and resolve conflicts**

```bash
git merge --no-ff refs/heads/shay/sweet-bell-f8e85a -m "Merge the pairing keyboard-clearance fix"
```

Resolve each conflicted file:
- **`src/pairing-home.tsx`:** keep this branch's retro file, then add the fix's wiring:
  - `useKeyboardClearance(pinInputRef)`.
  - `ref`, `onScroll` and `onLayout` on the `ScrollView`.
  - `ref`, `onLayout` and `testID="pairing-code"` on the code-panel `View`.
  - `onFocus` on `pin-input`.
- **`__tests__/App.test.tsx`** and **`__mocks__/react-native.js`:** keep both sides.
- **`CHANGELOG.md`:** keep both entries.

- [ ] **Step 3: Run every gate**

```bash
pnpm test && pnpm typecheck
(cd ../.. && pnpm check:ci && pnpm test:native)
```

Expected: everything passes.

- [ ] **Step 4: Commit the resolution** (`git commit --no-edit` if the merge stopped).

---

### Task 1: Theme tokens and standby colors

**Files:**
- Modify: `src/theme.ts`, `src/navigation-theme.ts`
- Test: `__tests__/theme.test.ts`, `__tests__/native-surfaces.test.tsx`, `__tests__/retro-primitives.test.tsx`, `__tests__/App.test.tsx`

**Interfaces:**
- Produces these `colors` keys:
  - `faceTop`, `faceBottom`, `faceHighlight`, `faceRow`
  - `grooveDark`, `grooveLight`
  - `keyTop`, `keyBottom`, `keyHighlight`
  - `capTop`, `capBottom`
- `navigationTheme.colors.card === colors.faceTop`.
- `screenHeaderOptions.headerShadowVisible === false`.

- [ ] **Step 1: Write the failing tests**

In `theme.test.ts`'s `toMatchObject`, change `liveDim` to `"#3a332a"` and `signOffText` to `"#4a4136"`, and add:

```ts
faceTop: "#1f1b16",
faceBottom: "#16130f",
faceHighlight: "rgba(255,236,200,0.07)",
faceRow: "rgba(42,38,31,0.55)",
grooveDark: "#0d0b09",
grooveLight: "rgba(255,236,200,0.06)",
keyTop: "#2a261f",
keyBottom: "#1d1a15",
keyHighlight: "rgba(255,236,200,0.08)",
capTop: "#8a8174",
capBottom: "#4e473d",
```

- In `native-surfaces.test.tsx`, expect `card: "#1f1b16"` and add `headerShadowVisible: false` to the `screenHeaderOptions` expectation.
- In `retro-primitives.test.tsx`, the unlit sign expects `color: "#4a4136"`.
- In `App.test.tsx`, both unlit-sign assertions (`#5a1814`) become `#4a4136`.

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- theme native-surfaces retro-primitives App`

Expected: FAIL on the new token values.

- [ ] **Step 3: Add the tokens to `colors`** and set `card: colors.faceTop` and `headerShadowVisible: false`.

- [ ] **Step 4: Run them and confirm they pass**

Same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): satin faceplate tokens and brown-gray standby`

---

### Task 2: The satin finish (`Faceplate`, `Groove`, `LedWindow`)

**Files:**
- Create:
  - `scripts/make-grain.mjs`
  - `assets/textures/grain.png`, `grain@2x.png`, `grain@3x.png`
  - `src/ui/faceplate.tsx`
  - `src/ui/led-window.tsx`
- Modify:
  - `src/ui/led-readout.tsx`: its well uses `wellStyle`
  - `__mocks__/react-native.js`: adds `Image: mock("Image")`
  - `jest.config.js`: maps `\\.(png)$` to `<rootDir>/__mocks__/file.js`
- Test: `__tests__/retro-primitives.test.tsx`

**Interfaces:**
- Produces:
  - `Faceplate({ children?: ReactNode; style?: StyleProp<ViewStyle>; testID?: string })`. It renders one hidden layer `View` with `testID="faceplate-finish"`.
  - `Groove({ testID?: string })`
  - `wellStyle: ViewStyle`
  - `LedWindow(props: ViewProps)`, which merges `wellStyle` and 12 × 14 pt padding under `props.style`.

- [ ] **Step 1: Write the generator**

`scripts/make-grain.mjs` writes `assets/textures/grain.png` (128 px), `grain@2x.png` (256 px) and `grain@3x.png` (384 px):
- 8-bit grayscale PNGs (color type 0).
- Each pixel is `Math.floor(rand() * 256)` from mulberry32 with seed `0x0a1a`, reseeded for each file.
- Filter byte 0 on each row, then `zlib.deflateSync`, with chunks checksummed by `zlib.crc32`.
- It uses `node:zlib` and `node:fs` only.

- [ ] **Step 2: Generate and check the tiles are reproducible**

```bash
node scripts/make-grain.mjs && git add assets/textures
node scripts/make-grain.mjs && git diff --exit-code assets/textures && file assets/textures/grain*.png
```

Expected: `git diff` exits 0, and `file` reports 128 × 128, 256 × 256 and 384 × 384 grayscale.

- [ ] **Step 3: Write the failing tests**

```tsx
test("faceplate: satin gradient, top highlight and a fine repeating grain, all hidden", () => {
  const r = create(<Faceplate testID="f">{null}</Faceplate>);
  const finish = hostById(r, "faceplate-finish");
  expect(finish.props).toMatchObject({
    pointerEvents: "none",
    accessibilityElementsHidden: true,
    importantForAccessibility: "no-hide-descendants",
  });
  expect(flat(finish.props.style).experimental_backgroundImage).toBe(
    "radial-gradient(ellipse 120% 45% at 50% 0%, rgba(255,236,200,0.07), transparent), linear-gradient(180deg, #1f1b16, #16130f)",
  );
  const grain = finish.find((n) => isHost(n, "Image"));
  expect(grain.props.resizeMode).toBe("repeat");
  expect(flat(grain.props.style).opacity).toBe(0.035);
});

test("groove: an engraved dark line over a faint light one", () => {
  const r = create(<Groove testID="g" />);
  expect(hostById(r, "g").props.accessibilityElementsHidden).toBe(true);
  expect(flat(hostById(r, "g").props.style)).toMatchObject({
    borderTopWidth: 1,
    borderTopColor: "#0d0b09",
    borderBottomWidth: 1,
    borderBottomColor: "rgba(255,236,200,0.06)",
  });
});

test("LED window: a recessed well with no border", () => {
  const r = create(<LedWindow testID="w" />);
  const style = flat(hostById(r, "w").props.style);
  expect(style).toMatchObject({
    backgroundColor: "#070605",
    borderRadius: 6,
    boxShadow: "inset 0 2px 6px rgba(0,0,0,0.7), 0 1px 0 rgba(255,236,200,0.06)",
    paddingVertical: 12,
    paddingHorizontal: 14,
  });
  expect(style.borderWidth).toBeUndefined();
});
```

In the existing LED readout test, add: the `led` host's flattened style has the `wellStyle` `boxShadow` and no `borderWidth`.

- [ ] **Step 4: Run them and confirm they fail**

Run: `pnpm test -- retro-primitives`

Expected: FAIL, because the modules don't exist yet.

- [ ] **Step 5: Implement `faceplate.tsx`, `led-window.tsx`, and the `wellStyle` change in `LedReadout`**

- `Faceplate` is a `View` (`flex: 1` with `style`).
- Its first child is the absolute-fill finish layer, with `backgroundColor: colors.faceBottom`, the gradient string, and the grain `Image` from `require("../../assets/textures/grain.png")` absolutely filling it.
- Then the children.
- `wellStyle` is `{ backgroundColor: colors.ledWell, borderRadius: radius.md, borderCurve: "continuous", boxShadow: <string above> }`.

- [ ] **Step 6: Run them and confirm they pass**

Run: `pnpm test`

Expected: PASS, with the full suite green.

- [ ] **Step 7: Commit** `feat(mobile): satin faceplate finish, grooves and LED wells`, including the script and the three PNGs.

---

### Task 3: `LedText`

**Files:**
- Modify: `src/ui/led-readout.tsx`
- Test: `__tests__/retro-primitives.test.tsx`

**Interfaces:**
- Produces:
  - `isDsegText(text: string): boolean`, true when `/^[\x20-\x7E]*$/` matches.
  - `LedText({ children: string; size: number; dim?: boolean; testID?: string })`. It renders one `Text` carrying the `testID`, with its children unchanged. Uppercasing is done by style, so `JSON.stringify` searches still find the original string.

- [ ] **Step 1: Write the failing tests**

```tsx
test("LED text: ASCII in 14-segment amber, uppercased by style, one line", () => {
  const r = create(<LedText size={18} testID="t">Living room</LedText>);
  const t = hostById(r, "t");
  expect(t.props.children).toBe("Living room");
  expect(t.props).toMatchObject({
    numberOfLines: 1,
    ellipsizeMode: "tail",
    adjustsFontSizeToFit: true,
    minimumFontScale: 0.75,
    maxFontSizeMultiplier: 1.3,
  });
  expect(flat(t.props.style)).toMatchObject({
    fontFamily: "DSEG14ClassicMini-Regular",
    fontSize: 18,
    textTransform: "uppercase",
    color: "#e0a24b",
  });
});

test("LED text: names DSEG14 cannot draw fall back to Plex Mono and still stay on one line", () => {
  const name = "Küche – HomePod 🎵 Wohnzimmer Lautsprecher";
  const t = hostById(create(<LedText size={18} testID="t">{name}</LedText>), "t");
  expect(flat(t.props.style).fontFamily).toBe("IBMPlexMono-Medium");
  expect(t.props.numberOfLines).toBe(1);
  expect(isDsegText("ON AIR 44.1K")).toBe(true);
  expect(isDsegText("Café")).toBe(false);
});

test("LED text: dim is muted with no glow", () => {
  const t = hostById(create(<LedText size={18} dim testID="t">Choose a source</LedText>), "t");
  expect(flat(t.props.style)).toMatchObject({ color: "#9a8f78", textShadowRadius: 0 });
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- retro-primitives`

Expected: FAIL, because `LedText` doesn't exist.

- [ ] **Step 3: Implement `LedText` and `isDsegText`**

Use the same glow as `LedReadout`: `textShadowColor` amber and `textShadowRadius` 6, or 0 when `dim`.

- [ ] **Step 4: Run them and confirm they pass**

Same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): one-line 14-segment LED text with a Plex Mono fallback`

---

### Task 4: `PanelKey`

**Files:**
- Create: `src/ui/panel-key.tsx`
- Test: `__tests__/panel-key.test.tsx`

**Interfaces:**
- Produces `keyFace({ pressed, lit }: { pressed: boolean; lit: boolean }): ViewStyle`.
- Produces `PanelKey` with these props:

```ts
{
  label?: string;
  icon?: IconName;
  iconSize?: number;
  shape?: "key" | "round";
  onPress: () => void;
  disabled?: boolean;
  busy?: boolean;
  indicator?: boolean;
  lit?: boolean;
  accessibilityLabel?: string; // defaults to label
  accessibilityHint?: string;
  accessibilityValue?: { text: string };
  style?: StyleProp<ViewStyle>; // layout only (flexBasis, width)
  testID: string;
}
```

- **`"key"`:** `minHeight: 56` and `borderRadius: radius.sm`.
- **`"round"`:** 36 × 36, `borderRadius: 18` and `hitSlop={4}`.
- **Accessibility:** role `button`, with `accessibilityState: { disabled, busy }`.
- **Indicator:** when set, a 6 × 6 amber dot with testID `${testID}-indicator`.

- [ ] **Step 1: Write the failing tests**

```tsx
test("key face: beveled gradient that inverts when pressed; lit keys are amber", () => {
  expect(keyFace({ pressed: false, lit: false })).toEqual({
    backgroundColor: "#1d1a15",
    experimental_backgroundImage: "linear-gradient(180deg, #2a261f, #1d1a15)",
    boxShadow: "inset 0 1px 0 rgba(255,236,200,0.08), 0 2px 0 #0a0908",
    transform: [{ translateY: 0 }],
  });
  expect(keyFace({ pressed: true, lit: false })).toEqual({
    backgroundColor: "#1d1a15",
    experimental_backgroundImage: "linear-gradient(180deg, #1d1a15, #2a261f)",
    boxShadow: "inset 0 1px 2px rgba(0,0,0,0.5)",
    transform: [{ translateY: 1 }],
  });
  expect(keyFace({ pressed: false, lit: true })).toEqual({
    backgroundColor: "#e0a24b",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.25), 0 2px 0 #0a0908",
    transform: [{ translateY: 0 }],
  });
});
```

Then four rendered tests (with the `create`, `hostById` and `flat` helpers, as in `retro-primitives`):
- **"a key is a labelled button with a capped mono caption":**
  - `accessibilityRole: "button"` and `accessibilityLabel: "Source"`.
  - `flat(style)` matches `{ minHeight: 56, borderRadius: 4 }`.
  - The label `Text` has `maxFontSizeMultiplier: 1.3`, with style `fontFamily: "IBMPlexMono-Medium"` and `color: "#f3ead4"`.
- **"a disabled key can't be pressed and dims":**
  - `onPress` is undefined.
  - `accessibilityState` is `{ disabled: true, busy: false }`.
  - `flat(style).opacity` is `0.45`.
- **"the indicator is an amber dot":** `${testID}-indicator` has style `{ width: 6, height: 6, backgroundColor: "#e0a24b" }`. Without `indicator`, it doesn't exist.
- **"a lit busy key shows a dark spinner instead of its caption":** with `lit busy`, the `ActivityIndicator` has `color "#141210"`, there is no label `Text`, and `accessibilityState.busy` is true.

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- panel-key`

Expected: FAIL, because the module doesn't exist.

- [ ] **Step 3: Implement `panel-key.tsx`**

- A `Pressable` whose style function returns `[shapeStyle, keyFace({ pressed, lit }), disabled && { opacity: 0.45 }, style]`.
- The label uses `type.label`, in `onAmber` when lit and `label` otherwise.
- The icon is a `NativeIcon` in the same colors.

- [ ] **Step 4: Run them and confirm they pass**

Same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): beveled faceplate keys`

---

### Task 5: `useFaderDraft` and `Fader`

**Files:**
- Create: `src/hooks/useFaderDraft.ts`, `src/ui/fader.tsx`
- Modify:
  - `src/native-fader.tsx`: uses the hook, with no behavior change
  - `__mocks__/react-native.js`: adds `PanResponder`
- Test: `__tests__/useFaderDraft.test.tsx`, `__tests__/fader.test.tsx`

**Interfaces:**
- Produces `useFaderDraft(p: { value: number; min: number; max: number; step: number; onChange: (v: number) => void | Promise<void> }): { draft: number; preview(next: number): void }`.
- Produces `dragValue(p: { start: number; delta: number; length: number; min: number; max: number; step: number }): number`. It returns `start` when `length <= 0`, and otherwise clamps and rounds `start + (delta / length) * (max - min)`.
- Produces `Fader`, whose props `FaderProps` are exported:

```ts
{
  orientation: "horizontal" | "vertical";
  value: number;
  min: number;
  max: number;
  step: number;
  origin: number;
  accessibilityStep: number;
  accessibilityLabel: string;
  formatValue: (v: number) => string;
  disabled?: boolean;
  length?: number; // vertical track length; horizontal measures itself
  renderReadout?: (draft: number) => React.ReactNode; // vertical only, drawn above the track
  testID: string;
  onChange: (v: number) => void | Promise<void>;
}
```

- **Host nodes:**
  - The root carries `testID`, the pan handlers and the accessibility props.
  - The track is `${testID}-track` and takes `onLayout`.
  - The fill is `${testID}-fill`, which is absent when disabled.
  - The cap is `${testID}-cap`.
- **Cap position:** it sits at a percentage of the track.
  - Horizontal: `left: \`${f * 100}%\``.
  - Vertical: `top: \`${(1 - f) * 100}%\``.
  - Here `f = (clamp(draft) - min) / (max - min)`.
- **Fill:** it spans the same percentages from `origin` to the value.

- [ ] **Step 1: Add `PanResponder` to the React Native mock**

It computes `dx`/`dy` from `nativeEvent.pageX`/`pageY` relative to the grant:

```js
const PanResponder = {
  create(config) {
    let start = { x: 0, y: 0 };
    const gesture = (e) => ({ dx: e.nativeEvent.pageX - start.x, dy: e.nativeEvent.pageY - start.y });
    return {
      panHandlers: {
        onStartShouldSetResponder: (e) => config.onStartShouldSetPanResponder?.(e, { dx: 0, dy: 0 }) ?? false,
        onResponderTerminationRequest: (e) => config.onPanResponderTerminationRequest?.(e, gesture(e)) ?? true,
        onResponderGrant: (e) => {
          start = { x: e.nativeEvent.pageX, y: e.nativeEvent.pageY };
          config.onPanResponderGrant?.(e, { dx: 0, dy: 0 });
        },
        onResponderMove: (e) => config.onPanResponderMove?.(e, gesture(e)),
        onResponderRelease: (e) => config.onPanResponderRelease?.(e, gesture(e)),
        onResponderTerminate: (e) => config.onPanResponderTerminate?.(e, gesture(e)),
      },
    };
  },
};
```

- [ ] **Step 2: Write the failing hook tests**

Port both cases from `__tests__/native-fader.test.tsx` to `renderHook` (`@testing-library/react-native`), with the same values and timings.

- `preview(70.4)` makes `draft` 70.
- After `rerender` with value 40, `draft` stays 70.
- After 180 ms, `onChange` is called with 70.
- After `rerender` with 45, `draft` stays 70. After 300 ms it is 45.
- After `rerender` with 20, `draft` is 20.
- The second case runs the same steps with `step: 0.5` and the settle window. After unmount, `onChange` is called twice in total.

- [ ] **Step 3: Write the failing `Fader` tests**

Use `react-test-renderer` with `node(r, id)` (the host whose `testID` is `id`), `touch(x, y = 0) = { nativeEvent: { pageX: x, pageY: y } }` and `layout(w, h) = { nativeEvent: { layout: { x: 0, y: 0, width: w, height: h } } }`. Create `const onChange = jest.fn()` at module scope, and in `beforeEach` call `onChange.mockReset()` and turn on fake timers.

```tsx
const volume = (over: Partial<FaderProps> = {}) => (
  <Fader orientation="horizontal" value={50} min={0} max={100} step={1} origin={0}
    accessibilityStep={5} accessibilityLabel="Volume" formatValue={String}
    testID="vol" onChange={onChange} {...over} />
);

test("drags relative to the touch, snapping and clamping", async () => {
  const r = create(volume());
  act(() => node(r, "vol-track").props.onLayout(layout(300, 44)));
  const root = node(r, "vol");
  act(() => root.props.onResponderGrant(touch(10)));
  expect(node(r, "vol").props.accessibilityValue.now).toBe(50); // no jump to the touch point
  act(() => root.props.onResponderMove(touch(79))); // +69 px of 300 → +23
  expect(node(r, "vol").props.accessibilityValue.now).toBe(73);
  act(() => root.props.onResponderMove(touch(600)));
  expect(node(r, "vol").props.accessibilityValue.now).toBe(100);
  act(() => root.props.onResponderRelease(touch(600)));
  await act(async () => { jest.advanceTimersByTime(180); });
  expect(onChange).toHaveBeenLastCalledWith(100);
});
```

Further tests, each with its exact assertions:
- **"vertical drags up to boost":**
  - EQ props: `min -12`, `max 12`, `step 0.5`, `origin 0`, `length 240`, and a `formatValue` that returns `"plus 2.5 decibels"` for 2.5.
  - Moving `touch(0, -25)` after `touch(0, 0)` makes `now` 2.5 and `text` "plus 2.5 decibels".
- **"fill runs from the origin":**
  - EQ value 6: the fill style has `{ top: "25%", height: "25%" }`.
  - EQ value −6: `{ top: "50%", height: "25%" }`.
  - Volume 50: `{ left: "0%", width: "50%" }`.
- **"a touch alone never changes the value":** grant, then release at the same point, then 500 ms. `onChange` is not called.
- **"claims the touch and refuses to hand it over":** `onStartShouldSetResponder(touch(0))` is `true` and `onResponderTerminationRequest(touch(0))` is `false`.
- **"accessibility: adjustable with steps":**
  - `accessibilityRole: "adjustable"`.
  - `accessibilityActions` is `[{ name: "increment" }, { name: "decrement" }]`.
  - `accessibilityValue` is `{ min: 0, max: 100, now: 50, text: "50" }`.
  - `onAccessibilityAction({ nativeEvent: { actionName: "increment" } })` makes `now` 55, and after 180 ms `onChange` is called with 55.
- **"disabled ignores touches and actions":**
  - `onStartShouldSetResponder` and `onAccessibilityAction` are undefined.
  - `accessibilityState` is `{ disabled: true }`.
  - `vol-fill` doesn't exist, and the `vol-cap` style has `opacity: 0.45`.
- **Review Focus 2, "a desktop value mid-drag never moves the cap":** grant at `touch(0)`, move to `touch(90)` (`now` 80), then rerender with `value={20}`. `now` stays 80.
- **Review Focus 3, "disabling mid-drag ignores further moves":**
  - Keep the `onResponderMove` reference, move to `touch(30)` (`now` 60), then rerender with `disabled`.
  - Calling the kept handler with `touch(150)` leaves the cap at `left: "60%"`.
  - After 500 ms, `onChange.mock.calls` is `[[60]]`.
- **Review Focus 4, "touches before layout are ignored; out-of-range values pin the cap":**
  - With no layout, a grant then a move to `touch(200)` leaves `now` at 50, and `Number.isFinite(now)`.
  - `create(volume({ value: 120 }))` gives a `vol-cap` style with `left: "100%"` and `now` 100.
- **`dragValue` unit:**
  - `dragValue({ start: 50, delta: 69, length: 300, min: 0, max: 100, step: 1 })` is 73.
  - With `length: 0` it is 50.
  - `{ start: 0, delta: -500, length: 240, min: -12, max: 12, step: 0.5 }` is −12.

- [ ] **Step 4: Run them and confirm they fail**

Run: `pnpm test -- useFaderDraft fader`

Expected: FAIL, because the modules don't exist.

- [ ] **Step 5: Implement**

- **`useFaderDraft`:** move `NativeFader`'s clamp, timers and refs into it unchanged. `NativeFader` then calls `const { draft, preview } = useFaderDraft(...)`.
- **`Fader`:**
  - `PanResponder.create` lives in a `useRef`, and its handlers read the latest `disabled`, `draft` and track length through refs.
  - The grant stores `start = draft`.
  - The track length is the `length` prop when vertical and the track's measured width (from `onLayout`) when horizontal. It is 0 until measured.
  - A move calls `preview(dragValue({ start, delta: horizontal ? dx : -dy, length, ... }))` unless the `disabled` ref is set.
  - Pan handlers and `onAccessibilityAction` are spread only when not disabled.
  - Geometry:
    - The slot is 6 pt wide, with `wellStyle`.
    - The fill is 3 pt wide and amber.
    - The cap is 40 × 20 (20 × 40 when horizontal), with `experimental_backgroundImage: "linear-gradient(180deg, #8a8174, #4e473d)"`, `boxShadow: "0 2px 3px rgba(0,0,0,0.6)"`, `borderRadius: radius.sm`, and a centered 1 pt `colors.label` index line.
  - The root is at least 44 pt across.

- [ ] **Step 6: Run them and confirm they pass**

Run: `pnpm test`

Expected: PASS. `native-fader.test.tsx` still passes unchanged.

- [ ] **Step 7: Commit** `feat(mobile): custom faders that commit like the native slider`

---

### Task 6: `homeStatus`, `formatRate` and `formatDb`

**Files:**
- Modify: `src/ui-state.ts`
- Create: `src/readouts.ts`
- Test: `__tests__/ui-state.test.ts`, `__tests__/readouts.test.ts`

**Interfaces:**
- Produces `type HomeStatus = { word: string; announcement: string; live: boolean }`.
- Produces `homeStatus(p: { uiState: UiState; outputPhase?: OutputPhase; firewallBlocked: boolean }): HomeStatus`. `restoring` and `pairing` fall through to the `STANDBY` row.
- Produces `formatRate(hz: number): { text: string; spoken: string }`.
- Produces `formatDb(db: number): { text: string; spoken: string }`.

- [ ] **Step 1: Write the failing tests**

```ts
test("home status: the LED word, announcement and lamp for every state", () => {
  const s = (uiState: UiState, extra: Partial<Parameters<typeof homeStatus>[0]> = {}) =>
    homeStatus({ uiState, firewallBlocked: false, ...extra });
  expect(s("live", { firewallBlocked: true })).toEqual({ word: "BLOCKED", announcement: "Firewall blocking audio", live: false });
  expect(s("live")).toEqual({ word: "ON AIR", announcement: "Audio is live", live: true });
  expect(s("starting")).toEqual({ word: "CONNECTING", announcement: "Status: Connecting speaker", live: false });
  expect(s("connecting")).toEqual({ word: "CONNECTING", announcement: "Status: Connecting", live: false });
  expect(s("reconnecting")).toEqual({ word: "RECONNECT", announcement: "Status: Reconnecting", live: false });
  expect(s("paused")).toEqual({ word: "PAUSED", announcement: "Desktop service is paused", live: false });
  expect(s("ready", { outputPhase: "failed" })).toEqual({ word: "FAILED", announcement: "Status: Speaker failed", live: false });
  expect(s("ready")).toEqual({ word: "STANDBY", announcement: "Status: Ready", live: false });
});
```

```ts
test("sample rate reads in kHz with at most one decimal", () => {
  expect(formatRate(44100)).toEqual({ text: "44.1K", spoken: "44.1 kilohertz stereo" });
  expect(formatRate(48000)).toEqual({ text: "48K", spoken: "48 kilohertz stereo" });
  expect(formatRate(88200).text).toBe("88.2K");
  expect(formatRate(96000).text).toBe("96K");
  expect(formatRate(0)).toEqual({ text: "--.-K", spoken: "sample rate unknown" });
});

test("decibels read signed on the LED and in words aloud", () => {
  expect(formatDb(3.5)).toEqual({ text: "+3.5", spoken: "plus 3.5 decibels" });
  expect(formatDb(0)).toEqual({ text: "0", spoken: "0 decibels" });
  expect(formatDb(-2)).toEqual({ text: "-2", spoken: "minus 2 decibels" });
  expect(formatDb(-12).text).toBe("-12");
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- ui-state readouts`

Expected: FAIL.

- [ ] **Step 3: Implement both modules**

`homeStatus` uses `describeUiState(uiState).statusAnnouncement` for the rows the spec marks "today's".

- [ ] **Step 4: Run them and confirm they pass**

Same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): LED status words and readout formats`

---

### Task 7: Keys replace the route card and the Sound menu

**Files:**
- Modify: `src/mixer-home.tsx`, `app/index.tsx`, `src/navigation-theme.ts`, `__mocks__/expo-router.js` (drops `"/sound"`), `src/settings-form.tsx` (delete `rowColors` if nothing uses it once Sound is gone)
- Delete: `app/sound.tsx`
- Test: `__tests__/App.test.tsx`, `__tests__/native-surfaces.test.tsx`

**Interfaces:**
- Consumes: `PanelKey` (Task 4).
- `MixerHome` props:
  - Drops `onOpenSound` and `onDisconnect`.
  - Adds `onOpenEqualizer: () => void`, `onOpenFormat: () => void` and `eqFlat: boolean`.
  - `index.tsx` passes `eqFlat={remote.gains.every((g) => g === 0)}` and pushes `/equalizer` and `/audio-format`.
- **Keys container:** `testID="mixer-keys"`, with `flexDirection: "row"`, `flexWrap: "wrap"` and `gap: 8`.
- **Key layout:** each key's `style` is `{ flexGrow: 1, flexBasis: grid ? "48%" : 0 }`, where `grid = fontScale > 1.3 || width < 340`. Read both from `useWindowDimensions`.
- **Key placement:** for now the keys sit where the Sound button was. Task 9 sets the final order.
- `stackScreens` becomes `equalizer`, `audio-format` and `connection`, with no `backTitle`.

- [ ] **Step 1: Update the existing tests to the new paths**

Follow the spec's mapping table. In the happy path:

```ts
await press("source-key");   // was source-row
await press("speaker-key");  // was output-row
await press("eq-key");       // was sound-settings-button + equalizer-link
// … EQ edits unchanged …
ReactTestRenderer.act(() => expoRouter.router.back());
await press("format-key");   // was audio-format-link
// … sample rate unchanged …
await press("native-back");  // one back now reaches the mixer (was two)
```

- **Paused test:** `eq-key` and `format-key` both have `onPress` undefined.
- **Other tests:**
  - The starting/failed test and the unpaired-AirPlay test: `output-row` becomes `speaker-key`.
  - The layout test's ID list: `source-key`, `speaker-key`, `volume-slider-native`, `eq-key` and `format-key`.
  - Delete the `live-dot` and `output-row` background assertions.
  - The firewall test asserts `flat(host("live-lamp").props.style).shadowOpacity === 0` instead.
- **`native-surfaces.test.tsx`:**
  - The names are `["equalizer", "audio-format", "connection"]`, and no `headerBackTitle` is `"Sound"`.
  - The sections test uses `AudioFormatScreen`, with headers `["SAMPLE RATE", "AUDIO PERFORMANCE"]`.

- [ ] **Step 2: Write the failing new tests** in `App.test.tsx`

```ts
test("the four mixer keys open the sheets and the sound screens", async () => {
  await renderApp();
  await pairWith();
  expect(exists("sound-settings-button")).toBe(false);
  expect(exists("disconnect-button")).toBe(false);
  await press("source-key");
  expect(exists("source-sheet")).toBe(true);
});

test("the EQ key says Custom with an amber dot when any band is moved", async () => {
  api.getEq.mockResolvedValue([0, 2, 0, 0, 0]);
  await renderApp();
  await pairWith();
  expect(host("eq-key").props.accessibilityValue).toEqual({ text: "Custom" });
  expect(exists("eq-key-indicator")).toBe(true);
});

test("keys wrap into a 2 × 2 grid at the largest text size", async () => {
  jest.mocked(useWindowDimensions).mockReturnValue({ width: 390, height: 844, scale: 3, fontScale: 1.6 });
  await renderApp();
  await pairWith();
  expect(flat(host("eq-key").props.style)).toMatchObject({ flexBasis: "48%" });
});
```

Also add a flat case: `getEq` all zeros gives `accessibilityValue` `{ text: "Flat" }` and no indicator.

- [ ] **Step 3: Run them and confirm they fail**

Run: `pnpm test -- App native-surfaces`

Expected: FAIL. `source-key`, `eq-key` and the rest don't exist, and `sound` is still a route.

- [ ] **Step 4: Implement**

- Add the keys, as in the spec's key table: hints, enabled rules, and `accessibilityValue` for EQ.
- Delete `RouteRow`, `RouteDivider` and the "Now streaming" `SectionLabel`.
- Delete the Sound and Disconnect pressables and `app/sound.tsx`.
- Update `stackScreens`, the router mock and `index.tsx`.

- [ ] **Step 5: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS.

- [ ] **Step 6: Commit** `feat(mobile): faceplate keys replace the route card and the Sound menu`

---

### Task 8: The LED display window replaces the status pill and lamp

**Files:**
- Create: `src/led-display.tsx`
- Modify: `src/mixer-home.tsx`, `app/index.tsx`, `src/ui/notice.tsx`, `src/ui/firewall-notice.tsx`
- Test: `__tests__/led-display.test.tsx`, `__tests__/App.test.tsx`, `__tests__/firewall-notice.test.tsx`

**Interfaces:**
- Consumes: `LedWindow` (Task 2), `LedText` (Task 3), `PanelKey` (Task 4), and `HomeStatus`, `homeStatus` and `formatRate` (Task 6).
- Produces `LedDisplay({ source: string; speaker: string; status: HomeStatus; rate: { text: string; spoken: string } })`.
  - `source` and `speaker` are `""` when none is chosen.
  - It renders `led-window` (accessible, role `text`), `led-source`, `led-speaker`, `live-lamp`, `led-status` and `led-format`.
- **Spoken summary:**
  - The form is `` `${status.announcement}. ${src}. ${spk}. ${cap(rate.spoken)}.` ``.
  - `src` is `` `Source ${source}` `` or `"No source chosen"`.
  - `spk` is `` `Speaker ${speaker}` `` or `"No speaker chosen"`.
  - `cap` uppercases the first letter.
- **Lamp:** an 8 × 8 dot.
  - Live: `backgroundColor: colors.accent`, `shadowColor: colors.accent` and `shadowOpacity: 0.6`.
  - Otherwise: `colors.liveDim` and `shadowOpacity: 0`.
- `MixerHome`'s `status` prop becomes `HomeStatus`, and it adds `outputSampleRate: number`.
- `index.tsx` computes `homeStatus({ uiState: remote.uiState, outputPhase: remote.activeOutput ? outputPhase(remote.activeOutput) : undefined, firewallBlocked: remote.firewallWarning != null })` and passes `outputSampleRate={remote.outputSampleRate}`. The firewall override inside `MixerHome` is deleted.

- [ ] **Step 1: Write the failing `led-display` tests**

```tsx
test("the window shows source, speaker, status and format, and reads as one summary", () => {
  const r = create(<LedDisplay source="Spotify" speaker="Living room"
    status={{ word: "ON AIR", announcement: "Audio is live", live: true }}
    rate={{ text: "44.1K", spoken: "44.1 kilohertz stereo" }} />);
  expect(hostById(r, "led-window").props).toMatchObject({
    accessible: true,
    accessibilityRole: "text",
    accessibilityLabel: "Audio is live. Source Spotify. Speaker Living room. 44.1 kilohertz stereo.",
  });
  expect(hostById(r, "led-source").props.children).toBe("Spotify");
  expect(hostById(r, "led-status").props.children).toBe("ON AIR");
  expect(hostById(r, "led-format").props.children).toBe("44.1K · ST");
  expect(flat(hostById(r, "live-lamp").props.style)).toMatchObject({
    backgroundColor: "#ff3b2a", shadowColor: "#ff3b2a", shadowOpacity: 0.6,
  });
});
```

More tests:
- **Empty rows:**
  - `source=""` gives `led-source` children `"Choose a source"` in color `#9a8f78`.
  - The label contains `"No source chosen"`.
  - `speaker=""` behaves the same way.
- **Standby lamp:** `backgroundColor: "#3a332a"` and `shadowOpacity: 0`.
- **Review Focus 1:** the speaker `"Küche – HomePod 🎵 Wohnzimmer Lautsprecher"` gives `led-speaker` `fontFamily: "IBMPlexMono-Medium"` and `numberOfLines: 1`.

- [ ] **Step 2: Update `App.test.tsx`**

Add the helper:

```ts
/** The first sentence of the LED window's summary: what the status pill used to announce. */
const announced = () => (host("led-window").props.accessibilityLabel as string).split(". ")[0];
```

- Every `host("status-pill").props.accessibilityLabel` becomes `announced()`, with the same expected strings.
- **Starting test:** `text().toContain("Connecting…")` becomes `host("led-status").props.children === "CONNECTING"`. Keep the "Speaker failed. Choose it again." check, now under `speaker-failed`.
- **Firewall test:** add `host("led-status").props.children === "BLOCKED"`.
- **Accessibility test:** the pill check becomes `led-window` with `accessible: true` and `accessibilityRole: "text"`.
- **Retro header and lamp tests:** use `live-lamp` `backgroundColor` and `shadowOpacity`, as in Step 1.
- **New test:** `getSampleRate` output 44100 gives `led-format` children `"44.1K · ST"`.

- [ ] **Step 3: Write the failing message tests**

- `mixer-error` (from a failed volume write) and `firewall-warning` have no `backgroundColor` and no `borderWidth`.
- `firewall-notice.test.tsx` adds the same assertion on `firewall-warning`.

- [ ] **Step 4: Run them and confirm they fail**

Run: `pnpm test -- led-display App firewall-notice`

Expected: FAIL.

- [ ] **Step 5: Implement**

- Add `LedDisplay`.
- The header drops the pill and its bottom border, and ⋯ becomes `PanelKey shape="round" icon={icons.more}` with testID `more-button` and label "More options".
- Delete the lamp block.
- Messages render under the window:
  - `Notice`: no box.
  - The "Speaker failed. Choose it again." text: testID `speaker-failed`, `type.body` in `secondaryLabel`.
  - `FirewallNotice`: no box. Its toggle and command are unchanged.

- [ ] **Step 6: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS.

- [ ] **Step 7: Commit** `feat(mobile): an LED display window replaces the status pill and lamp`

---

### Task 9: Volume fader, CD keys and the faceplate on home

**Files:**
- Modify: `src/mixer-home.tsx`
- Test: `__tests__/App.test.tsx`

**Interfaces:**
- Consumes: `Faceplate` and `Groove` (Task 2), `Fader` (Task 5) and `PanelKey` (Task 4).
- **Faceplate:** `MixerHome`'s root becomes `<Faceplate testID="mixer-screen">`.
- **Phone order:**
  1. Header
  2. Window
  3. Messages
  4. CD
  5. Groove
  6. Volume
  7. Groove
  8. Keys
- **Wide layout:**
  - Left column: window, messages and CD.
  - Right column: volume and keys.
  - The layout test IDs are unchanged.
- **Volume:**
  - The volume `Fader` uses `testID="volume-slider"`, `orientation="horizontal"`, `min 0`, `max 100`, `step 1`, `origin 0` and `accessibilityStep 5`.
  - `formatValue = (v) => String(Math.round(v))` and `disabled={volumeDisabled}`.
  - It drops the `Host` wrapper.
  - The readout is `LedReadout size={shortWide ? 36 : 44}` inside the existing hidden wrapper, still showing the `volume` prop.
  - The `0` / `50` / `100` scale `Text`s each carry `accessibilityElementsHidden` and `importantForAccessibility="no"`, so `numbersShown()` never counts them.
- **CD transport:**
  - `cd-transport` is a plain `View` with no `Card`.
  - `cd-prev`, `cd-play` and `cd-next` are icon `PanelKey`s, 64 pt wide.

- [ ] **Step 1: Update the existing tests**

Add the helper:

```ts
/** Moves a fader by one accessibility step, as VoiceOver's swipe up / down does. */
function stepFader(id: string, action: "increment" | "decrement", times = 1) {
  for (let i = 0; i < times; i += 1) {
    ReactTestRenderer.act(() =>
      host(id).props.onAccessibilityAction({ nativeEvent: { actionName: action } }),
    );
  }
}
```

- **Volume writes:** `onValueChange(73)` becomes `stepFader("volume-slider", "increment")`, with the expected value 55. That applies to both the happy path and the rollback test, where the rollback now asserts `not.toContain(55)`.
- **Disabled volume:** `volume-slider-native … onValueChange` undefined becomes `host("volume-slider").props.accessibilityState` equal to `{ disabled: true }`.
- **Layout test:** `volume-slider-native` becomes `volume-slider`.
- **CD style test:** `cd-play` style matches `{ backgroundColor: "#1d1a15", experimental_backgroundImage: "linear-gradient(180deg, #2a261f, #1d1a15)" }`.

- [ ] **Step 2: Write the failing success-criterion test**

```ts
test("the mixer is one faceplate: no bordered boxes anywhere on home", async () => {
  await renderApp();
  await pairWith();
  const boxes = host("mixer-screen").findAll(
    (n) =>
      ["View", "Pressable"].includes(n.type as string) &&
      Number(flat(n.props.style).borderWidth ?? 0) > 0,
  );
  // The ONAIR sign is a lamp housing, not a group box; it is the only bordered shape left.
  expect(boxes.map((b) => b.props.testID)).toEqual(["mixer-sign"]);
  expect(exists("faceplate-finish")).toBe(true);
});
```

- [ ] **Step 3: Run them and confirm they fail**

Run: `pnpm test -- App`

Expected: FAIL. `volume-slider` has no accessibility actions yet, and `Card`s still have borders.

- [ ] **Step 4: Implement the layout above**

Remove the `Card` and `Host` imports and the `TransportButton` component.

- [ ] **Step 5: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS.

- [ ] **Step 6: Commit** `feat(mobile): LED volume over a faceplate fader, CD keys, one faceplate`

---

### Task 10: The vertical-fader Equalizer

**Files:**
- Create: `src/eq-panel.tsx`
- Modify: `app/equalizer.tsx`
- Delete: `src/native-fader.tsx`, `__tests__/native-fader.test.tsx`
- Test: `__tests__/eq-panel.test.tsx`, `__tests__/equalizer-reset.test.tsx` (from Task 0), `__tests__/native-surfaces.test.tsx`, `__tests__/App.test.tsx`

**Interfaces:**
- Consumes: `Fader` (Task 5), `formatDb` (Task 6), `LedWindow` and `LedText` (Tasks 2 and 3), and `Faceplate`.
- Produces `eqTrackLength(rowHeight: number): number`, which is `clamp(rowHeight - 96, 140, 320)`.
- Produces `EqPanel({ gains: number[]; disabled: boolean; onChangeBand: (index: number, value: number) => void })`.
- **Bands:** `[{ freq: "60", name: "Bass", spoken: "Bass, 60 hertz" }, { freq: "250", name: "Warmth", … }, { freq: "1K", name: "Mid", spoken: "Midrange, 1 kilohertz" }, { freq: "4K", name: "Pres", spoken: "Presence, 4 kilohertz" }, { freq: "12K", name: "Air", spoken: "Air, 12 kilohertz" }]`.
- **Faders:** each is `testID="eq-band-{i}"`, vertical, with `min -12`, `max 12`, `step 0.5`, `origin 0` and `accessibilityStep 1`.
  - `formatValue = (v) => formatDb(v).spoken`.
  - `renderReadout = (d) => <LedReadout value={formatDb(d).text} digits={4} size={16} accessibilityLabel="" testID="eq-band-{i}-readout" />`, inside a hidden wrapper.
- **Curve readout:** `eq-curve` (`LedText`) reads `FLAT` or `CUSTOM`.
- **Footnote:** "Drag up to boost, down to cut. 0 dB leaves the sound unchanged."
- **Screen:** `app/equalizer.tsx` renders `<Faceplate testID="equalizer-screen">` with an `EqPanel`. It keeps the iOS `Stack.Toolbar` Reset and the Android `headerRight` `eq-reset` exactly as Task 0 left them.

- [ ] **Step 1: Write the failing `eq-panel` tests**

They mock nothing: `EqPanel` is pure props. Use the `create`, `hostById` and `flat` helpers from `retro-primitives`, and `textsIn` from `App.test`.

```tsx
test("five vertical faders with spoken band names and LED dB readouts", () => {
  const r = create(<EqPanel gains={[3.5, 0, -2, 0, 12]} disabled={false} onChangeBand={jest.fn()} />);
  expect(hostById(r, "eq-band-0").props).toMatchObject({
    accessibilityRole: "adjustable",
    accessibilityLabel: "Bass, 60 hertz",
    accessibilityValue: { min: -12, max: 12, now: 3.5, text: "plus 3.5 decibels" },
  });
  expect(hostById(r, "eq-band-2").props.accessibilityValue.text).toBe("minus 2 decibels");
  expect(textsIn(hostById(r, "eq-band-0-readout")).map((t) => t.props.children)).toContain("+3.5");
  expect(hostById(r, "eq-curve").props.children).toBe("CUSTOM");
});
```

More tests:
- **"flat curve":** all zeros gives `eq-curve` `"FLAT"`.
- **"an increment commits the stepped band":** with fake timers, an increment on `eq-band-2` from 0, then 180 ms, gives `onChangeBand` called with `(2, 1)`.
- **"disabled":** every `eq-band-{i}` has `accessibilityState` `{ disabled: true }`.
- **"track length fills the row within bounds":** `eqTrackLength(100)` is 140, `(300)` is 204 and `(1000)` is 320.
- **"scale and 0 dB line are decoration":** the `eq-scale` wrapper has `accessibilityElementsHidden: true`.
- **"footnote":** the exact footnote string renders.

- [ ] **Step 2: Update the other tests**

- **`App.test.tsx` happy path:**
  - Replace the two `onValueChange` calls with `stepFader("eq-band-0", "increment")` and `stepFader("eq-band-1", "increment", 2)`.
  - Expect `setEq` to be last called with `[1, 2, 0, 0, 0]`.
  - Replace `"Bass · 60 Hz"` with `host("eq-band-0").props.accessibilityLabel === "Bass, 60 hertz"`.
- **`native-surfaces.test.tsx`:** drop `EqualizerScreen` from the "every settings screen" loop.
- **`equalizer-reset.test.tsx`:** must keep passing. Change only assertions that relied on the old form layout, and keep every Reset rule.

- [ ] **Step 3: Run them and confirm they fail**

Run: `pnpm test -- eq-panel App native-surfaces equalizer-reset`

Expected: FAIL, because `eq-panel` doesn't exist.

- [ ] **Step 4: Implement**

- **`EqPanel`:** the curve readout, then a row with `flex: 1` and `onLayout`.
  - The row holds the hidden `eq-scale` column (`+12`, `+6`, `0`, `−6` and `−12` in `type.label` `tertiaryLabel`, plus the 1 pt `grooveLight` 0 dB line).
  - Then five equal columns: fader, frequency (mono) and name (sans, 12 pt).
  - Then the footnote.
- **`length`:** `eqTrackLength(measuredRowHeight)`.
- **Cleanup:** delete `NativeFader` and its test, now that nothing imports it.

- [ ] **Step 5: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS.

- [ ] **Step 6: Commit** `feat(mobile): five vertical EQ faders with LED dB readouts`

---

### Task 11: Pairing on the faceplate

**Files:**
- Modify: `src/pairing-home.tsx`
- Delete: `src/ui/card.tsx`, `src/ui/icon-tile.tsx`, and their two tests in `retro-primitives.test.tsx`, but only if `grep -rn "ui/card\|ui/icon-tile" src app` finds nothing else.
- Test: `__tests__/App.test.tsx`

**Interfaces:**
- Consumes: `Faceplate`, `Groove`, `wellStyle`, and `PanelKey` with `lit` and `busy`.
- **Discovered rows:** no `IconTile`. A bare `NativeIcon` sits in its place, and rows are separated by `Groove`.
  - The selected row has `backgroundColor: colors.faceRow` and no border.
  - Its label and amber check are unchanged.
- **Cards:** `Card` wrappers become plain `View`s with the same gaps.
- **Host field:** `host-input` uses `[wellStyle, { fontFamily: fonts.mono, color: colors.label, fontSize: 17, minHeight: 50, paddingHorizontal: 14 }]`.
- **PIN field:** `pin-input` uses `wellStyle`. It keeps its LED font, its amber color, `backgroundColor: colors.ledWell`, and every prop from Task 0.
- **Connect:** `pair-button` becomes `<PanelKey label="Connect" lit={lit} busy={pairing} disabled={!canPair} onPress={onPair} testID="pair-button" />`.
- **Unchanged:** every test ID and the `useKeyboardClearance` wiring.

- [ ] **Step 1: Update the existing tests**

- **Pairing style test:**
  - The unlit `pair-button` `backgroundColor` is `"#1d1a15"`.
  - Lit is still `"#e0a24b"`, and the label is still `"#141210"`.
  - The busy spinner is still `"#141210"`.
- **Write a failing test:**

```ts
test("pairing is one faceplate: no bordered boxes", async () => {
  await renderApp();
  await press("manual-setup-button");
  const boxes = host("pairing-screen").findAll(
    (n) =>
      ["View", "Pressable", "TextInput"].includes(n.type as string) &&
      Number(flat(n.props.style).borderWidth ?? 0) > 0,
  );
  expect(boxes.map((b) => b.props.testID)).toEqual(["remote-title"]); // only the sign's housing
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- App`

Expected: FAIL. The `Card` borders and the input borders are still there.

- [ ] **Step 3: Implement**

Wrap the `ScrollView` in a `<Faceplate>` inside the `KeyboardAvoidingView`, which keeps `testID="pairing-screen"`. Then apply the changes above.

- [ ] **Step 4: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS, with all pairing tests green.

- [ ] **Step 5: Commit** `feat(mobile): pairing on the satin faceplate`

---

### Task 12: Settings forms and the restoring screen on the faceplate

**Files:**
- Modify: `src/settings-form.tsx`, `app/index.tsx` (`RestoringScreen`)
- Test: `__tests__/native-surfaces.test.tsx`

**Interfaces:**
- **`SettingsForm`:** it renders `<Faceplate>` around its `Host`, and the `Host` has `style={{ flex: 1, backgroundColor: "transparent" }}`.
  - The iOS form modifiers are `[scrollContentBackground("hidden"), background("transparent")]`.
  - `sectionModifiers` is `[listRowBackground(colors.faceRow)]`.
- **Android:** check `node_modules/@expo/ui` for a `FieldGroup` container-color prop on Android.
  - If one exists, set it to `"transparent"`.
  - If not, leave Android unchanged and add "Android settings forms keep a flat background" to the PR's known gaps.
- **`RestoringScreen`:** `<SettingsForm testID="pairing-restore">` with the same section.

- [ ] **Step 1: Write the failing tests**

- The settings-form test expects modifiers containing `{ $type: "scrollContentBackground", visible: "hidden" }` and `{ $type: "background", color: "transparent" }`, plus a `faceplate-finish` host.
- The sections test expects `{ $type: "listRowBackground", color: "rgba(42,38,31,0.55)" }`.

- [ ] **Step 2: Run them and confirm they fail**

Run: `pnpm test -- native-surfaces`

Expected: FAIL.

- [ ] **Step 3: Implement the interfaces above**

- [ ] **Step 4: Run them and confirm they pass**

Run: `pnpm test && pnpm typecheck`

Expected: PASS.

- [ ] **Step 5: Commit** `feat(mobile): settings forms sit on the satin faceplate`

---

### Task 13: Gates, device pass and docs

**Files:**
- Modify: `docs/screenshots/mobile.png`, `CHANGELOG.md`
- Modify if tuned: `src/theme.ts` and the spec's token tables

- [ ] **Step 1: Run every gate**

```bash
pnpm test && pnpm typecheck
(cd ../.. && pnpm check:ci && pnpm test:native)
```

Expected: all pass.

- [ ] **Step 2: Do the iOS 27 simulator pass against the mock core**

Use ports 47995 and Metro 8089, because other sessions hold 47990 and 8081–8083.

```bash
ON_AIR_MOCK=1 PORT=47995 /Users/shay/Projects/oss/on-air/target/debug/on-air-core
```

Then run the Debug build with `-RCT_jsLocation localhost:8089`, PIN 123456. Check each item:
- Pairing, with the keyboard lift.
- Every key.
- Volume drag, confirmed with `on-air status --json --base http://127.0.0.1:47995`.
- Each EQ band dragged and confirmed the same way.
- Reset.
- VoiceOver swipe up and down on volume and on one band.
- The largest text size.
- Gradients, grain and the lamp glow render.

- [ ] **Step 3: Do the Android emulator pass with the same flows**

Also check that a drag on the volume fader doesn't scroll the home page, and how the form backgrounds look.

- [ ] **Step 4: Apply the fallback or tuning if needed**

- If a gradient misrenders, pre-render the faceplate layers to PNGs, keeping the `Faceplate` API.
- If token values were tuned on device, update `theme.ts`, `theme.test.ts` and the spec's tables together.

- [ ] **Step 5: Update the docs**

- Replace `docs/screenshots/mobile.png` with a live-state iOS capture.
- Add a "Changed" entry under `[Unreleased]` in `CHANGELOG.md`: "Phone remote: the mixer is one LED faceplate, with a display window, a fader for volume, keys for source, speaker, EQ and format, and five vertical EQ faders. The Sound menu is gone."

- [ ] **Step 6: Commit** `docs(mobile): faceplate screenshot and changelog`
