# Faceplate pickers: source, speaker and pairing sheets

Date: 2026-10-23
Status: design approved in conversation, pending spec review
Builds on: the LED faceplate redesign (`2026-10-22-mobile-led-faceplate-redesign-design.md`) and the iPhone Duo work (`2026-10-22-mobile-iphone-duo-design.md`)
Branch: `feat/mobile-faceplate-pickers`, from `feat/mobile-iphone-duo` (e461b4e)

## Goal

The phone remote's mixer home reads as one piece of hardware: a satin
faceplate, LED windows, mono captions and bevelled keys. The three sheets it
opens do not. They are stock native sheets, with a system-font title, a bare
menu link for the source, and amber capsule buttons with truncated copy for
the speakers. The retro theme only tinted them amber (082d68b).

This redesign gives the source picker, the speaker picker and the speaker
pairing sheet the home page's design quality.

Success means:

- Opening any of the three sheets feels like part of the same device.
- Every speaker state still reads at a glance: Live, Connecting…, Failed · Retry,
  Desktop only, Pair, Connect. Names wrap; nothing truncates.
- Behaviour is unchanged: the same session calls, the same close-on-success,
  the same pair-request routing. No core or API changes.
- It works at default and at the largest text size, with VoiceOver and
  TalkBack, on the iPhone 17 Pro, the folded iPhone Duo and Android.

## Decisions

- **The owner chose, in conversation:**
  - Scope: the source, speaker and pairing sheets. The Audio format and
    Connection forms stay native forms.
  - Direction A, "channel strips", from three mockups (A channel strips,
    B selector keys, C menu display).
  - The approved mockup: `.superpowers/brainstorm/72318-1792724800/content/sheets-a.html`
    (local to the authoring worktree; `.superpowers/` is not committed).
  - Approach 1: keep the native `BottomSheet` and put faceplate React Native
    views inside it.
- **Why not B or C:** selector keys leave no room for Connecting… or
  Failed · Retry, and wrap long names in a grid. A segment-type menu forces
  short upper-case names and is hard to read with VoiceOver.
- **Why not formSheet routes or a custom panel:** routes reshape navigation
  (the pairing sheet would become a route with params, inside the paired-only
  `Stack.Protected`). A custom panel would reimplement drag, focus trapping,
  the VoiceOver escape gesture and Android back.
- **React Native inside a native sheet.** The Duo spec avoided React Native
  content inside a SwiftUI host (`ArrangementView`), mainly because the custom
  faders could fight SwiftUI for touches. The sheets hold no faders, only
  taps and one text field. Expo UI 58 documents `BottomSheet` children as React
  Native views on every platform. If the device check finds touch or keyboard
  trouble that cannot be fixed inside the sheet, the fallback is formSheet
  routes, with the same components.

## Design

### Shared pieces (`src/ui/`)

- **`Lamp`**: a 7 pt round lamp with four states.
  - `off`: `liveDim`, no glow.
  - `amber`: `amber` with a soft amber glow.
  - `live`: `accent` red with a red glow. Red stays reserved for live audio,
    as in the theme.
  - `error`: `onError` rose.
  - The LED display's inline `live-lamp` is replaced by `Lamp`, with no
    visual change.
- **`ChannelStrip`**: one tappable row.
  - Layout: a lamp, then a name (Plex Sans medium, 15 pt, wraps to two lines,
    `maxFontSizeMultiplier` 1.3), a mono caption under it (`type.label`
    style, tertiary ink), and an action word on the right (mono,
    `maxFontSizeMultiplier` 1.3).
  - Surface: `faceRow` with a `separator` hairline and radius 8, at least
    `layout.minTarget` tall.
  - Tones:

    | Tone | Lamp | Strip | Action ink |
    | --- | --- | --- | --- |
    | `idle` | off | `faceRow` | amber |
    | `selected` | amber | `faceRow`, `key` border | `amberBright` |
    | `connecting` | amber | `faceRow` | amber |
    | `live` | live | `selectedSurface`, `signLitLine` border | `accent` |
    | `failed` | error | `errorSurface`, `errorBorder` border | `onError` |
    | `unavailable` | off | `faceRow`, 45% opacity | tertiary |

  - Accessibility: role button. The label is "name, caption", the value is
    the action word, and the state carries `selected`, `busy` (connecting) and
    `disabled`. The pressed state darkens the strip, as `PanelKey` does.
- **`LedCodeInput`**: the LED number field from the first-run pairing screen.
  It's a `TextInput` in a `wellStyle` well, in the LED font with amber digits,
  ghost `8`s as the placeholder, an amber cursor, the number pad and
  `maxFontSizeMultiplier` 1.3. Props: `length`, `value`, `onChangeText`,
  `testID`, plus a forwarded ref. `pairing-home.tsx` uses it with length 6,
  with no visual change.

### `SheetPanel` (`src/sheets/sheet-panel.tsx`)

- Wraps `BottomSheet` with `sheetProps(spec)`, `contentPadding={0}` and a
  `containerColor` of `faceTop` on both platforms.
- Draws the faceplate surface (the `Faceplate` gradient and finish) behind
  its children, with 16 pt sides and the bottom safe-area inset.
- The header row holds a mono caption (`SOURCE`, `SPEAKER`, `PAIR SPEAKER`).
  The source and speaker sheets add a small `Done` `PanelKey` on the right;
  the pair sheet has its own Cancel key instead. The caption has
  `accessibilityRole="header"`.
- iOS keeps the dark colour scheme and amber tint modifiers, so the system
  drag indicator reads on the faceplate.

### Source sheet

- **LED window:** a lamp (amber when a source is chosen, off otherwise) and
  the chosen input's label in LED type. With none chosen it shows ghost
  `NO SOURCE`.
- **A groove, then one `ChannelStrip` per input:**
  - Caption: `CAPTURE DEVICE`.
  - Tones: `selected` with the action "Selected" for the active input;
    `connecting` with "Switching…" while `busyTarget` names it; `idle` with
    "Select" otherwise.
  - Tapping calls `pickInput(name)` and closes the sheet on success, as now.
  - Strips are disabled when controls are disabled or another switch is
    running.
- **Empty:** the window shows ghost `NO SOURCE` and the footnote reads
  "No capture devices are available."
- **Footnote otherwise:** "The Mac captures this device and streams it to the
  speaker." (Plex Sans 12, tertiary.)
- **Height:** `160 + inputs × 66` pt, at least 280, and full height above
  five inputs, with the strips in a `ScrollView`.

### Speaker sheet

- **LED window:**
  - The active output's name, with its lamp and a right-hand caption: `LIVE`
    in red, or `CONNECTING…` in amber.
  - With no output, it shows ghost `NO SPEAKER`.
  - While the list is empty, it shows ghost `SCANNING` and the footnote
    reads "Waiting for speakers on the LAN…".
- **One `ChannelStrip` per output:**
  - The caption is "Stereo pair · Sonos" style, as now.
  - The action word comes from the existing `outputAction()`.
  - A new pure `outputTone()` takes the same inputs and returns the tone:

    | Condition | Tone |
    | --- | --- |
    | desktop-only AirPlay | `unavailable` |
    | working, or selected and starting | `connecting` |
    | selected and failed | `failed` |
    | selected and live | `live` |
    | otherwise | `idle` |

  - Tapping keeps today's routing: unpaired speakers that need pairing open
    the pair sheet; others `activate()`, closing on success.
- **Add Bluetooth speaker:** a full-width `PanelKey`, "+ Add Bluetooth
  speaker", below the strips. Behaviour is unchanged.
- **AirPlay footnote:** in `avroute-picker` mode, "AirPlay speakers are picked
  from the desktop."
- **Height:** today's rule, recomputed for 66 pt strips: `200 + outputs × 66`,
  at least 300, and full height above five outputs, with the strips in a
  `ScrollView`.

### Pair speaker sheet

- **Content:**
  - The header caption `PAIR SPEAKER`, then the speaker's name (Plex Sans
    semibold, 18).
  - A footnote: AirPlay reads "Enter the code the speaker shows. No code?
    Leave it blank."; Bluetooth keeps "Confirm pairing on the speaker or in
    the computer's Bluetooth settings, then continue."
  - AirPlay only: a centred `LedCodeInput` of length `DEVICE_PIN_LENGTH` (8),
    digits only, as now.
- **Keys:** `Cancel` (one third) and a lit amber `Pair and connect` (two
  thirds), which reads `Pairing…` and disables both keys while
  `devicePairing`.
- **Behaviour is unchanged:** `pairDevice(target, pin)`, closing on success.

### Android

The same React Native content draws inside Compose's `ModalBottomSheet`. The
`containerColor` is `faceTop`, and the drag indicator stays on.

## Testing

Tests are written first, with the existing React Native Testing Library and
`react-test-renderer` setup. With React Native children, the sheets render
as plain React Native in tests.

- **`Lamp` and `ChannelStrip`:** each tone's lamp, surface and ink;
  accessibility label, value and state; disabled strips ignore presses; names
  are not limited to one line.
- **`outputTone()`:** the table above, row by row. `outputAction()` keeps its
  tests.
- **Source sheet:**
  - One strip per input, with the active one `selected`.
  - A tap calls `pickInput` and closes on success, but not on failure.
  - The busy strip reads "Switching…".
  - Disabled controls disable the strips.
  - The empty state and the LED window text.
- **Speaker sheet:**
  - Every state's strip.
  - Pair routing for an unpaired speaker that needs pairing.
  - Activate, then close on success.
  - Add Bluetooth.
  - The AirPlay footnote, and the empty SCANNING state.
- **Pair sheet:**
  - Digits only, up to 8.
  - Submit passes the code.
  - `Pairing…` disables both keys.
  - Bluetooth has no code field.
- **Existing tests:** `sheet-props` tests change to the new padding, container
  colour and heights. App-level tests that open the sheets use the new testIDs.
- **Device check:** on the iPhone 17 Pro, the folded iPhone Duo and the
  Android emulator, at default and at the largest text size.
  - All three sheets, against the mock core.
  - Source and speaker picks, as confirmed by `on-air status --json`.
  - The pair sheet's code field stays above the keyboard.
  - VoiceOver reads a strip as "Mock Sonos, Sonos, Live, button".

## Out of scope

- The Audio format and Connection forms, and the "More options" menu.
- Any change to the core, the control client, or what each pick does.
- New speaker features, such as grouping, volume per speaker, or forgetting
  a pairing.

## Risks

- **Keyboard over the code field.** If the native sheet does not lift React
  Native content above the keyboard, the AirPlay pair sheet takes a taller
  detent, enough to keep the field visible.
- **Touch or focus quirks in the hosted content.** These are fixed in place
  if possible. Otherwise the fallback in Decisions applies.
- **Release timing.** This branch starts from the Duo branch. Whether it joins
  1.0.0 or a later build is the owner's call at release time.
