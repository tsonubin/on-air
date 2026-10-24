# iPhone Duo System Bars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Equalizer's Reset, the mixer's "More options" and the pickers' Done/Cancel into iOS system bar items, so the folded iPhone Duo shows them in its vertical strip; then merge PR #35.

**Architecture:**
- Reset and More options become `Stack.Toolbar.Button`s with an SF Symbol and a title.
- The three pickers move from Expo UI's `BottomSheet` to expo-router routes presented as native form sheets. Their faceplate content from PR #35 is unchanged.
- iOS gets the native header and toolbar. Android, which has no vertical strip, keeps its in-content controls.

**Tech Stack:** Expo SDK 58, expo-router 58 (`Stack.Toolbar`, form-sheet presentation), React Native 0.88, Jest 30 with the repo's `__mocks__/`.

**Spec:** `docs/superpowers/specs/2026-10-24-mobile-duo-system-bars-design.md` (builds on `2026-10-23-mobile-faceplate-pickers-design.md`).

## Global Constraints

- **Toolbar items, iOS:** every item has both an SF Symbol `icon` and a title (its children), plus an `accessibilityLabel`. Apple doesn't move title-only items into the vertical strip.
- **Icons and copy, verbatim:**
  - Reset: `arrow.counterclockwise`, title "Reset", label "Reset equalizer".
  - More options: `ellipsis`, title and label "More options".
  - Done: `checkmark`, "Done".
  - Cancel: `xmark`, "Cancel".
- **Placement:** close/cancel items go `placement="left"`; Done, Reset and More options go `placement="right"`.
- **Android:** keeps today's controls (the in-content "…" key, the caption-and-Done row, the in-content Cancel key and the header Reset caption).
- **Behaviour is unchanged:** the same `useRemoteSession()` calls, closing on success, and the unpaired-speaker hand-over to pairing. No core or API changes; no new dependencies.
- **Kept testIDs:** `source-sheet`, `output-sheet`, `pair-sheet`, `input-<name>`, `output-<transport>-<id>`, `add-bluetooth`, `device-pin-input`, `device-pair-submit`, `source-led`, `speaker-led`, `speaker-led-state`.
  - `device-pair-cancel` and `<sheet>-done` remain on Android only.
  - `more-button` remains on Android only.
- **Gates** (from `apps/mobile`): `pnpm test`, `pnpm run typecheck`, `pnpm exec biome check src __tests__ app __mocks__`. Biome's one existing warning in `src/hooks/useDesktopConnection.ts` stays.
- **Commits and Metro:** stage explicit paths, no attribution lines, never force-push. Run Metro without `CI=1`.

## Review Focus

1. **A pick that finishes after its sheet was swiped away** must not go back from whatever screen is showing then. Test in Task 4.
2. **The pair route opened without its params** (a stale history entry) goes back instead of rendering a blank target. Test in Task 6.
3. **"More options" appears only on the paired mixer**, never on the pairing or restoring screen. Test in Task 3.
4. **Unpairing while a picker is open** returns to pairing, through the paired-only `Stack.Protected`, not to a dead sheet. Test in Task 5.
5. **On the pair sheet, Cancel is disabled while pairing on both platforms.** On iOS that's the toolbar item; on Android, the key. Test in Task 6.

---

### Task 1: Sync with `main` and prove the bars on the folded Duo

**Files:** whatever the merge touches. Expected: `apps/mobile/__tests__/App.test.tsx`, `apps/mobile/src/mixer-home.tsx`, `apps/mobile/app.json`.

- [ ] **Step 1: Merge `main` into `feat/mobile-faceplate-pickers`.** Use the app's `sync_with_base_branch` if this worktree's base is `main`; otherwise run `git merge --no-ff origin/main`. Resolve conflicts by keeping both sides' tests.
- [ ] **Step 2: Run the gates.** Expected: all pass, with `main`'s key-grid tests included.
- [ ] **Step 3: Commit the merge** if the sync tool didn't, then push the branch.
- [ ] **Step 4: Spike, a throwaway that is never committed.**
  - In `app/index.tsx`, on iOS, render the mixer header and a test toolbar:

    ```tsx
    <Stack.Screen options={{ headerShown: true, headerTransparent: true, headerTitle: "" }} />
    <Stack.Toolbar placement="right">
      <Stack.Toolbar.Button icon="ellipsis" accessibilityLabel="More options" onPress={…}>More options</Stack.Toolbar.Button>
    </Stack.Toolbar>
    ```

  - Add a scratch route `app/spike-sheet.tsx`: `presentation: "formSheet"`, `headerShown: true`, with a right `checkmark` "Done" toolbar button.
  - Run it on the folded Duo (`1497A209-2326-4F44-9C2B-EA9EA3771150`) and on the iPhone 17 Pro.
  - Expected:
    - folded Duo: "…" sits in the strip; the sheet's Done sits in the strip;
    - iPhone: both sit in the horizontal bar;
    - note whether the transparent header shows a scroll-edge effect over the faceplate.
- [ ] **Step 5: Revert the spike and record the result in the ledger.** If the form-sheet Done does not move into the strip, stop and tell the owner. The spec's fallback is Expo UI sheets with no in-content Done.

### Task 2: Equalizer Reset gets an icon

**Files:**
- Modify: `apps/mobile/app/equalizer.tsx` (the iOS `Stack.Toolbar.Button`)
- Test: `apps/mobile/__tests__/equalizer-reset.test.tsx` ("iOS keeps Reset in the native toolbar")

- [ ] **Step 1: Write the failing test.** Extend the existing iOS test:

  ```tsx
  expect(button.props).toMatchObject({
    icon: "arrow.counterclockwise",
    accessibilityLabel: "Reset equalizer",
    children: "Reset",
  });
  ```

- [ ] **Step 2: Run it and confirm it fails.** Run: `pnpm exec jest __tests__/equalizer-reset.test.tsx`. Expected: FAIL on `icon`.
- [ ] **Step 3: Add `icon="arrow.counterclockwise"` and `accessibilityLabel="Reset equalizer"`** to the iOS button. The title stays as its children.
- [ ] **Step 4: Run the test and the full suite.** Expected: PASS.
- [ ] **Step 5: Commit** `fix(mobile): Reset has an icon so the folded Duo can rail it`.

### Task 3: Mixer "More options" in the iOS toolbar

**Files:**
- Modify:
  - `apps/mobile/app/index.tsx` (`MixerScreen`)
  - `apps/mobile/src/mixer-home.tsx` (`Header`; `onOpenMore` becomes optional)
  - `apps/mobile/__mocks__/expo-router.js` (`Stack.Screen` also renders its options)
- Test: `apps/mobile/__tests__/App.test.tsx`

**Interfaces:**
- Produces:
  - `MixerHome` prop `onOpenMore?: () => void`. When it's undefined, the header has no "…" key.
  - Mock: `Stack.Screen` renders a `NativeScreenOptions` host with `options`, plus its `headerRight` as now.

- [ ] **Step 1: Write the failing tests** in `App.test.tsx`.
  - Add an `openMore()` helper that presses `toolbarButton("More options")` on iOS, and replace the eight `press("more-button")` calls with it.
  - The new tests:

  ```tsx
  test("iOS puts More options in the mixer's toolbar, not the content", async () => {
    await renderApp(); await pairWith();
    expect(exists("more-button")).toBe(false);
    expect(toolbarButton("More options").props).toMatchObject({ icon: "ellipsis", children: "More options" });
    const options = tree!.root.findAll((n) => (n.type as unknown) === "NativeScreenOptions").at(-1)!.props.options;
    expect(options).toMatchObject({ headerShown: true, headerTransparent: true, headerTitle: "" });
    await openMore();
    expect(exists("connection-screen")).toBe(true);
  });
  test("Android keeps the in-content More key", async () => {
    (Platform as { OS: string }).OS = "android"; // restore in finally
    await renderApp(); await pairWith();
    expect(tree!.root.findAll((n) => (n.type as unknown) === "NativeToolbarButton" && n.props.accessibilityLabel === "More options")).toHaveLength(0);
    await press("more-button");
    expect(exists("connection-screen")).toBe(true);
  });
  test("More options appears only on the paired mixer", async () => {
    await renderApp(); // pairing screen
    expect(tree!.root.findAll((n) => (n.type as unknown) === "NativeToolbarButton" && n.props.accessibilityLabel === "More options")).toHaveLength(0);
  });
  ```

  - The Connection screen's testID is `connection-screen`.
- [ ] **Step 2: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/App.test.tsx`. Expected: the three new tests FAIL.
- [ ] **Step 3: Implement.**
  - On iOS, `MixerScreen` renders `<Stack.Screen options={{ headerShown: true, headerTransparent: true, headerTitle: "" }} />` and a right `Stack.Toolbar.Button` with `icon="ellipsis"`, title and label "More options", pushing `/connection`.
  - It passes `onOpenMore` only on Android.
  - If Task 1's spike saw a scroll-edge effect, add `scrollEdgeEffects: { top: "hidden" }` to those options and to the test's expectation.
- [ ] **Step 4: Run the full suite.** Expected: PASS.
- [ ] **Step 5: Commit** `fix(mobile): More options moves to the iOS toolbar`.

### Revision (2026-10-25): Tasks 4–6 replaced

The Task 1 spike showed a form-sheet route's Done stays in a horizontal bar on the folded Duo. The owner chose the spec's fallback, so Tasks 4–6 below are not run. In their place:

**Task 4′: iOS sheets drop the in-content Done.**
- File: `apps/mobile/src/sheets/sheet-panel.tsx`.
- Tests: `apps/mobile/__tests__/picker-sheets.test.tsx`, plus `App.test.tsx` for any test that presses `source-sheet-done` or `output-sheet-done`.
- The change: `SheetPanel` renders its Done key only on Android. The caption row stays on both platforms.
- Failing tests first:
  - iOS: `p-done` doesn't exist, and `p-caption` does.
  - Android: `p-done` exists and calls `onClose`.
  - The sheet's `onDismiss` still calls `onClose`, since swipe-down is now iOS's only close.
- Commit: `fix(mobile): iOS picker sheets close by swipe, with nothing beside the Duo rail`.

Tasks 7–8 run as written, without the route checks.

### Task 4: Sheet routes, `SheetPanel` without `BottomSheet`, and the Source route

**Files:**
- Create: `apps/mobile/app/source.tsx`, `apps/mobile/src/sheets/use-close-sheet.ts`
- Modify:
  - `apps/mobile/src/sheets/sheet-panel.tsx` (rewrite)
  - `apps/mobile/src/sheets/source-sheet.tsx`
  - `apps/mobile/src/navigation-theme.ts` (`sheetScreen()`, `stackScreens`)
  - `apps/mobile/app/index.tsx`
  - `apps/mobile/__mocks__/expo-router.js`
- Delete: `apps/mobile/src/sheets/sheet-props.ts`, along with its `pure-helpers` block and the `native-surfaces` sheet test.
- Test: `apps/mobile/__tests__/picker-sheets.test.tsx`, `native-surfaces.test.tsx`, `App.test.tsx`

**Interfaces:**
- Produces:
  - `sheetScreen(title: string, detents: number[] | "fitToContents")`, which returns `{ ...titledScreen(title), presentation: "formSheet", sheetGrabberVisible: true, sheetAllowedDetents: detents, headerShown: Platform.OS === "ios" }`.
  - `SheetPanel({ caption, onClose, done = true, testID, children })`.
    - It is a `Faceplate` (flex 1) with a padded, scrollable-capable body and `testID` on the plate.
    - On iOS, when `done`, it renders `<Stack.Toolbar placement="right"><Stack.Toolbar.Button icon="checkmark" accessibilityLabel="Done" onPress={onClose}>Done</Stack.Toolbar.Button></Stack.Toolbar>`, with no in-content caption.
    - On Android it renders today's caption and Done row, with `${testID}-caption` and `${testID}-done`.
  - `useCloseSheet(): () => void`, which calls `router.back()` only while the calling component is mounted.
  - Mock router:
    - `push` and `replace` accept a string or `{ pathname, params }` and store `pathname?query`.
    - `useLocalSearchParams()` returns the current query as an object.
    - `useNavigation` is not needed.
    - Routes `/source`, `/speaker`, `/pair-speaker` resolve by pathname.

- [ ] **Step 1: Write the failing tests.**
  - `native-surfaces.test.tsx`:
    - `stackScreens` names are `["equalizer", "audio-format", "connection", "source", "speaker", "pair-speaker"]`.
    - `source` has `presentation: "formSheet"`, `sheetGrabberVisible: true`, `sheetAllowedDetents: [0.5, 1]` and `headerTitle: "SOURCE"`.
    - On Android, `headerShown` is false.
  - `picker-sheets.test.tsx`:
    - Replace the `SheetPanel` tests:
      - iOS: a `NativeToolbarButton` labelled "Done" with `icon: "checkmark"` and `children: "Done"`, which calls `onClose`; no `p-caption`.
      - Android: `p-caption` (role header) and `p-done`.
      - `done={false}`: no Done on either platform.
      - The rail test keeps `paddingRight: 84`.
    - Drop the `ExpoBottomSheet`, `RNHostView` and `sheetProps` assertions.
    - Review Focus 1:

      ```tsx
      test("a pick that lands after the sheet was swiped away does not go back", async () => {
        let resolve!: (ok: boolean) => void;
        mockSession = session({ pickInput: jest.fn(() => new Promise<boolean>((r) => (resolve = r))) });
        const back = jest.spyOn(router, "back");
        const r = create(<SourceRoute />);
        void press(hostById(r, "input-mic"));
        ReactTestRenderer.act(() => r.unmount());
        await ReactTestRenderer.act(async () => resolve(true));
        expect(back).not.toHaveBeenCalled();
      });
      ```

  - `App.test.tsx`: the source flow stays as it is (press `source-key`, press `input-PipeWire Sound Server`, `source-sheet` gone). Add that `source-sheet` exists after `source-key`, and that iOS's toolbar Done returns to the mixer.
- [ ] **Step 2: Run them and confirm they fail.** Run: `pnpm exec jest __tests__/picker-sheets.test.tsx __tests__/native-surfaces.test.tsx __tests__/App.test.tsx`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Write `sheetScreen` and add the three entries to `stackScreens`, with detents `[0.5, 1]` for source, `[0.65, 1]` for speaker, and `"fitToContents"` for pair-speaker.
  - Rewrite `SheetPanel`, and add `useCloseSheet` (a mounted ref around `router.back`).
  - `SourceSheet` takes its `onClose` from the route.
  - `app/source.tsx` renders `<SourceSheet onClose={useCloseSheet()} />`.
  - `MixerScreen`'s `onChangeInput` pushes `/source`.
  - Delete `sheet-props.ts` and its tests.
  - Extend the router mock.
  - Drop the `spec` prop from all three sheets' `SheetPanel` calls, so `OutputSheet` and `DevicePairSheet` compile against the new `SheetPanel`. They are routed in Tasks 5–6; until then `app/index.tsx` keeps rendering them inline.
- [ ] **Step 4: Run the full suite and typecheck.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(mobile): the source picker as a native form-sheet route`.

### Task 5: Speaker route

**Files:**
- Create: `apps/mobile/app/speaker.tsx`
- Modify: `apps/mobile/app/index.tsx` (removes the `sheet` and `pairTarget` state)
- Test: `picker-sheets.test.tsx`, `App.test.tsx`

**Interfaces:**
- Consumes: `SheetPanel`, `useCloseSheet` and `sheetScreen` (Task 4).
- Produces: `app/speaker.tsx` renders `<OutputSheet onClose={close} onPairRequest={(o) => router.replace({ pathname: "/pair-speaker", params: { transport: o.transport, id: o.id, name: o.name } })} />`.

- [ ] **Step 1: Write the failing tests** in `App.test.tsx`.
  - `speaker-key` shows `output-sheet`.
  - Pressing the unpaired AirPlay speaker leaves the route at `/pair-speaker?transport=airplay&id=ap-locked&name=…` with `pair-sheet` shown and `output-sheet` gone. (The existing AirPlay pairing test keeps its other assertions.)
  - Review Focus 4: with `/speaker` open, forgetting the desktop (or any unpair path the suite already drives) returns to the pairing screen, with no `output-sheet`.
- [ ] **Step 2: Run it and confirm it fails.** Expected: FAIL.
- [ ] **Step 3: Implement.** `app/speaker.tsx`; `MixerScreen`'s `onChangeOutput` pushes `/speaker`; delete the `sheet` and `pairTarget` state and the inline sheets.
- [ ] **Step 4: Run the full suite.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(mobile): the speaker picker as a native form-sheet route`.

### Task 6: Pair speaker route

**Files:**
- Create: `apps/mobile/app/pair-speaker.tsx`
- Modify: `apps/mobile/src/sheets/device-pair-sheet.tsx`
- Test: `picker-sheets.test.tsx`, `App.test.tsx`

**Interfaces:**
- Consumes: `useLocalSearchParams<{ transport?: string; id?: string; name?: string }>()`, `SheetPanel` with `done={false}`, and `useCloseSheet`.
- Produces: on iOS, `DevicePairSheet` renders `<Stack.Toolbar placement="left"><Stack.Toolbar.Button icon="xmark" accessibilityLabel="Cancel" disabled={devicePairing} onPress={onClose}>Cancel</Stack.Toolbar.Button></Stack.Toolbar>`, and "Pair and connect" fills the key row. Android keeps both in-content keys.

- [ ] **Step 1: Write the failing tests** in `picker-sheets.test.tsx`.
  - iOS:
    - the "Cancel" toolbar button has `icon: "xmark"` and `children: "Cancel"`, and calls `onClose`;
    - there is no `device-pair-cancel`;
    - with `devicePairing: true` the toolbar Cancel has `disabled: true` (Review Focus 5).
  - Android: `device-pair-cancel` exists, and has `onPress` undefined while pairing.
  - Review Focus 2:

    ```tsx
    test("the pair route without params goes back", () => {
      router.push("/pair-speaker"); // no query
      const back = jest.spyOn(router, "back");
      create(<PairSpeakerRoute />);
      expect(back).toHaveBeenCalledTimes(1);
    });
    ```

  - The existing pair tests (digits, submit, Bluetooth) keep passing.
- [ ] **Step 2: Run them and confirm they fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `app/pair-speaker.tsx` reads the params.
  - If `transport`, `id` or `name` is missing, it calls `router.back()` in an effect and renders nothing.
  - Otherwise it renders `<DevicePairSheet target={…} onClose={useCloseSheet()} />`.
- [ ] **Step 4: Run the full suite and typecheck.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(mobile): speaker pairing as a native form-sheet route`.

### Task 7: Device pass

**Files:** none planned. Each finding is a failing test, a fix, then a commit.

- [ ] **Step 1: Start the services.** Start the mock core with `ON_AIR_MOCK=1 PORT=47995 <repo>/target/debug/on-air-core` (PIN 123456), and run Metro in the background on a free port without `CI=1`. A rebuild is not needed: this is JS-only, with no native changes.
- [ ] **Step 2: Folded Duo.**
  - The mixer: "…" sits in the strip.
  - The EQ: Back on top, then Reset, in the strip.
  - Source and Speaker: Done in the strip, and the strips scroll.
  - Pair: Cancel at the top of the strip; the code field stays above the keyboard.
  - Picks are confirmed with `on-air status --json --base http://127.0.0.1:47995`.
- [ ] **Step 3: iPhone 17 Pro** at default and largest text.
  - The same items sit in the horizontal bar.
  - Tune `sheetAllowedDetents` if a sheet opens too short or tall; update the test and record a ruling.
  - Turn off a scroll-edge effect over the mixer if one shows.
- [ ] **Step 4: Android emulator.** The "…" key, the caption-and-Done rows and the pair keys work as before; back and swipe close the sheets; a long list scrolls.
- [ ] **Step 5: Record the results in the ledger.** Reset each simulator's text size to `large`.

### Task 8: Final review, push and merge #35

- [ ] **Step 1: Final review.** One reviewer on the most capable model reviews the whole branch from `0c11712` (or the merge base with `main`), with this plan's Review Focus. Fix Critical and Important findings test-first; defer the minor ones to the ledger.
- [ ] **Step 2: Push** `feat/mobile-faceplate-pickers`, and update PR #35's description: the sheets are now form-sheet routes, the controls sit in the system bars, and the testing and known gaps are updated.
- [ ] **Step 3: Wait for CI.** Auto-fix reports failures. Merge only when every check passes and the PR is mergeable: `gh pr merge 35 --merge`, the repo's merge-commit style. Then confirm `main` has the merge.
