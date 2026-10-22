# Mobile LED faceplate redesign

Date: 2026-10-22
Status: approved in conversation, pending spec review
Builds on: `2026-10-18-mobile-retro-theme-design.md` (PR #33)
Branch: `feat/mobile-led-faceplate`, stacked on `feat/mobile-retro-theme`

## Goal

The retro theme reskinned the existing layout. The home screen still reads as
a settings app in costume: bordered rounded cards, a status pill, a separate
lamp block, and a Sound menu one level down. The owner said the design still
feels off, and that they love the LED volume readout.

This redesign makes the mixer home and the equalizer look and work like one
piece of hardware:

- one continuous satin faceplate
- a recessed LED display window
- a large LED volume over a real fader
- beveled keys
- five vertical EQ faders

Success means:

- The home screen has no card boxes. The only recessed shapes are LED
  windows, fader slots and text fields.
- Every current feature is still reachable, with the same rules for when it
  is enabled.
- Volume and EQ changes commit exactly as they do today.
- VoiceOver and TalkBack can read and adjust every control.

## Decisions

- **Direction:** an LED display-window home and a vertical-fader equalizer,
  chosen from mockups.
- **Fewer boxes:** in the owner's words, "less border + bg rounded boxes, if
  you need to add, at least add more textures". The texture has to be fine,
  not rough.
- **Finish A, satin anodized** (described below).
- **Custom faders:** this reverses the 2026-10-18 rule "no custom-drawn
  faders or knobs". Volume and the EQ bands become custom-drawn faders.
  Sheets, pickers and settings forms stay native.
- **Red means live only:** the unlit sign, the off lamp and the idle dots turn
  warm brown-gray instead of dark red.
- **No new native modules:** React Native 0.86 draws gradients and inset
  shadows itself (`experimental_backgroundImage` and `boxShadow`; the New
  Architecture is on for both platforms). Touch uses React Native's
  `PanResponder`. No `pod install` and no Gradle changes.
- **Delivery:** a separate branch and PR stacked on PR #33, so the reskin can
  merge on its own.

### Choices this spec makes (not discussed in conversation)

- **Faders move relative to the finger.** Touching anywhere on a fader grabs
  it, and the value then moves with the drag. A touch never jumps the value
  to the touch point. This refines "touch anywhere on the track to jump
  there" from the approved Section 2: a jump could send volume from 20 to 100
  on a stray tap, and today's iOS slider never jumps.
- **The format corner** shows the output sample rate and `ST`, because the
  pipeline is always stereo.
- **Ghost segments** stay behind fixed-width numbers only (volume, CD
  counter, EQ dB). Text rows have none.
- **Settings-form rows** become translucent so the grain shows through.

## Scope

**In scope:**

- the mixer home
- the Equalizer screen
- the pairing screen's surfaces
- the settings forms' background (Audio format, Connection)
- native header colors
- removing the Sound screen
- standby colors and new theme tokens

**Out of scope:**

- Bottom sheets (unchanged).
- The remote session, control client, desktop app and core. No API or data
  changes.
- New features or controls.
- Android light-mode handling (`shay/elastic-edison-283413`).
- The iOS 27 keyboard fix for Connect (`shay/sweet-bell-f8e85a`).

## Finish A: satin anodized

A full-screen `Faceplate` view draws three layers behind its content, from
bottom to top:

1. **Linear gradient,** top to bottom, `faceTop` to `faceBottom`.
2. **Top highlight:** a radial gradient, an ellipse 120% wide and 45% tall,
   centered at the top edge, from `faceHighlight` to transparent.
3. **Grain:** a tile image repeated across the view at opacity 0.035.

Rules:

- No borders around groups. Groups are separated by space and an engraved
  `Groove`.
- No screws, grilles, rivets or brushed lines.
- **Recessed:** LED windows, fader slots and text fields only.
- **Raised:** keys and fader caps only.
- **Labels** are silkscreened: `type.label` text in `tertiaryLabel`, printed
  directly on the faceplate.

The grain tile ships as `assets/textures/grain.png`, `grain@2x.png` and
`grain@3x.png`. The 1x tile is 128 × 128 pt, and each scale has one noise pixel
per device pixel, so the grain stays fine on 3x screens. A checked-in script,
`apps/mobile/scripts/make-grain.mjs`, generates all three. It uses `node:zlib`
only and a fixed seed, so the output is reproducible.

**Fallback:** `experimental_backgroundImage` is still marked experimental. If
it renders wrong on a platform during the device pass, layers 1 and 2 ship as
pre-rendered PNGs. The look and the `Faceplate` API stay the same.

## Tokens (`src/theme.ts`)

New colors:

| Token | Value | Use |
| --- | --- | --- |
| `faceTop` | `#1f1b16` | Faceplate gradient top, native header background |
| `faceBottom` | `#16130f` | Faceplate gradient bottom |
| `faceHighlight` | `rgba(255,236,200,0.07)` | Top highlight |
| `faceRow` | `rgba(42,38,31,0.55)` | Translucent settings-form rows |
| `grooveDark` | `#0d0b09` | Groove upper line |
| `grooveLight` | `rgba(255,236,200,0.06)` | Groove lower line, well bottom edge, 0 dB line |
| `keyTop` | `#2a261f` | Key face gradient top |
| `keyBottom` | `#1d1a15` | Key face gradient bottom |
| `keyHighlight` | `rgba(255,236,200,0.08)` | Key top edge |
| `capTop` | `#8a8174` | Fader cap gradient top |
| `capBottom` | `#4e473d` | Fader cap gradient bottom |

Changed colors:

| Token | Old | New |
| --- | --- | --- |
| `signOffText` | `#5a1814` | `#4a4136` |
| `liveDim` | `#6a2a22` | `#3a332a` |

The navigation theme's `card` becomes `faceTop`, and headers set
`headerShadowVisible: false`, so a native header blends into the faceplate
below it. Values may be tuned during the device pass. Any change is written
back into this table.

## Components (`src/ui/`)

### `Faceplate`

`Faceplate({ children, style, testID })`. The three layers sit absolutely
behind the children with `pointerEvents="none"`, and are hidden from
accessibility.

### `Groove`

A full-width engraved line: 1 pt `grooveDark` over 1 pt `grooveLight`, hidden
from accessibility.

### `LedWindow`

A recessed well for LED text and text fields:

- `ledWell` background, 6 pt corners, 12 × 14 pt padding.
- An inset shadow (`0 2 6 rgba(0,0,0,0.7)`) and a 1 pt `grooveLight` bottom
  edge.
- Children are free.

### `LedText`

A one-line 14-segment text row, added to `led-readout.tsx` next to
`LedReadout`:

- **Font:** text in printable ASCII renders uppercased in DSEG14. Text with any
  other character (accents, CJK, emoji) renders uppercased in Plex Mono Medium,
  because DSEG14 can't draw it.
- **Fitting:** it shrinks to 75% before truncating at the end, and caps at
  1.3× text size.
- **Color:** amber, or `tertiaryLabel` when `dim`.

### `PanelKey`

`PanelKey({ label, onPress, disabled, indicator, lit, icon, accessibilityLabel, accessibilityHint, accessibilityValue, testID })`.

- **Size:** at least 56 pt tall, with 4 pt corners.
- **Face:**
  - a `keyTop` to `keyBottom` gradient
  - a 1 pt `keyHighlight` top edge
  - a `0 2 0` drop shadow in `background`
- **Pressed:** the gradient reverses, the drop shadow goes away, and the key
  moves down 1 pt.
- **Disabled:** 0.45 opacity.
- **Label:** `type.label` in `label` color, capped at 1.3×.
- **`indicator`:** a 6 pt amber dot above the label.
- **`lit`:** an amber face with `onAmber` text. Pairing's Connect key uses it.
- **Accessibility:** role `button`, with `disabled` in its state.

### `useFaderDraft` (`src/hooks/useFaderDraft.ts`)

The preview, commit-after-pause, and settle logic moves out of
`NativeFader` unchanged. It keeps:

- the clamp and step rounding
- `controlCommitMs`
- `interactionSettleMs`
- ignoring incoming values while dragging
- applying the latest value once settled

It returns `{ draft, preview }`. Every case in `native-fader.test.tsx` is
ported to the hook. `NativeFader` and its test file are then deleted, since
nothing else uses them.

### `Fader` (`src/ui/fader.tsx`)

```ts
Fader({
  orientation: "horizontal" | "vertical",
  value, min, max, step,
  origin,            // where the amber fill starts: 0 for volume and EQ
  accessibilityStep, // volume 5, EQ 1
  accessibilityLabel,
  formatValue,       // spoken value text
  disabled,
  length,            // track length in pt (vertical only)
  testID,
  onChange,
})
```

**Drawing:**

- **Slot:** recessed and 6 pt wide, `ledWell` with an inset shadow.
- **Fill:** amber, 3 pt wide, running inside the slot from `origin` to the
  current value.
- **Cap:**
  - 40 × 20 pt (20 × 40 pt when horizontal)
  - a `capTop` to `capBottom` gradient
  - a 1 pt cream index line across its middle
  - a `0 2 3 rgba(0,0,0,0.6)` shadow
- **Disabled:** the fill turns off and the cap dims to 0.45.

**Touch:**

- The whole fader box is the touch target, at least 44 pt across.
- A `PanResponder` claims the touch on start and refuses termination
  requests, so a parent `ScrollView` can't steal a drag.
- The new value is `startValue + delta / trackLength × (max − min)`, rounded
  to `step` and clamped, and is fed to `preview`.
- A disabled fader never becomes the responder.

**Accessibility:**

- `accessible`, role `adjustable`, with the label.
- `accessibilityValue` is `{ min, max, now, text: formatValue(draft) }`.
- The `increment` and `decrement` actions move by `accessibilityStep`
  through `preview`, so they commit after the same pause.
- `disabled` goes in its state.

**Test IDs:** `testID` and `${testID}-track`.

## Mixer home (`src/mixer-home.tsx`)

### Phone layout (narrower than 700 pt)

The screen sits on a `Faceplate`. The `ScrollView`, safe-area handling and
960 pt maximum width stay as they are today. Top to bottom:

1. **Header row:** the Sign on the left, lit when live, and ⋯ on the right
   (`more-button`). The ⋯ button is a 36 pt round key with the same gradient
   and bevel as `PanelKey`, and 4 pt of hit slop brings it to 44. There is no
   status pill and no bottom border.
2. **LED window** (`led-window`), one accessible element:
   - **Source row:** a silkscreen `SOURCE` over the source name in `LedText`
     (`led-source`). With no source it shows `Choose a source`, dimmed.
   - **Speaker row:** a silkscreen `SPEAKER` over the speaker name in
     `LedText` (`led-speaker`). With no speaker it shows `Choose a speaker`,
     dimmed.
   - **Bottom strip, left:** the lamp (`live-lamp`), an 8 pt dot in `accent`
     with a red glow when live and `liveDim` otherwise, then the status word
     (`led-status`) in DSEG14 at 14 pt.
   - **Bottom strip, right:** the format corner (`led-format`), for example
     `44.1K · ST`, in `tertiaryLabel`.
   - **Spoken summary,** for example "Audio is live. Source Spotify. Speaker
     Living room. 44.1 kilohertz stereo."
3. **Messages under the window,** as plain faceplate text with no box:
   - The error or warning (`mixer-error`, role `alert`, `onError` color).
   - "Speaker failed. Choose it again." when the speaker failed.
   - The firewall message, the Firewall help toggle and the selectable repair
     command (`firewall-warning`, `firewall-command`).

   `Notice` and `FirewallNotice` lose their box and keep their content and
   behavior.
4. **CD transport,** only when a disc is present:
   - a small `LedWindow` with the track counter (`cd-track`)
   - previous, play/pause and next as `PanelKey`s with icons, keeping today's
     test IDs
5. A `Groove`.
6. **Volume:**
   - A silkscreen `VOLUME`.
   - A small `LedWindow` holding the three-digit `LedReadout` at 44 pt
     (`volume-readout`).
   - Then a full-width horizontal `Fader` (`volume-slider`) over a printed
     `0 · 50 · 100` scale.
   - It is disabled exactly when it is today (`volumeDisabled`). The readout
     dims while disabled.
7. A `Groove`.
8. **Keys,** four equal `PanelKey`s with 8 pt gaps. They wrap to a 2 × 2 grid
   when the font scale is above 1.3 or the width is under 340 pt.

| Key | testID | Action | Enabled | Extras |
| --- | --- | --- | --- | --- |
| SOURCE | `source-key` | Source sheet | Always, as the source row is today | Hint "Choose what the desktop captures" |
| SPEAKER | `speaker-key` | Output sheet | Always, as the speaker row is today | Hint "Choose where the audio plays" |
| EQ | `eq-key` | Push `/equalizer` | When `soundDisabled` is false | Indicator dot and value "Custom" when any band ≠ 0, otherwise value "Flat" |
| FORMAT | `format-key` | Push `/audio-format` | When `soundDisabled` is false | None |

The home screen has no Disconnect button. Connection's "Forget desktop" does
the same thing.

### Wide (700 pt and up) and short-wide (also under 600 pt tall)

The two columns stay, with their existing layout test IDs:

- **Left (flex 1.08):** the LED window, messages and CD transport.
- **Right (flex 0.92):** volume and the keys.

Short-wide uses 12 pt gaps and a 36 pt volume readout.

### Status word

A new pure function in `src/ui-state.ts`,
`homeStatus({ uiState, outputPhase, firewallBlocked })`, returns
`{ word, announcement, live }`. "Today's `statusAnnouncement`" means
`describeUiState(uiState).statusAnnouncement`. The first matching row wins:

| Condition | Word | Lamp and sign | Announcement |
| --- | --- | --- | --- |
| Firewall blocked | `BLOCKED` | Off | "Firewall blocking audio" |
| `live` | `ON AIR` | Lit | "Audio is live" |
| `starting` or `connecting` | `CONNECTING` | Off | Today's `statusAnnouncement` |
| `reconnecting` | `RECONNECT` | Off | Today's `statusAnnouncement` |
| `paused` | `PAUSED` | Off | "Desktop service is paused" |
| `ready` and the speaker failed | `FAILED` | Off | "Status: Speaker failed" |
| `ready` | `STANDBY` | Off | Today's `statusAnnouncement` |

### Format corner

The format corner reads the session's `outputSampleRate`:

- 44100 shows `44.1K` and 48000 shows `48K`. Rates are written in kHz with at
  most one decimal.
- An unknown rate (0) shows `--.-K` and is spoken as "sample rate unknown".

### Props

`MixerHome` changes its props:

- **Drops:** `onOpenSound` and `onDisconnect`.
- **Replaces** `status: HeaderStatus` with `status: HomeStatus`, the output of
  `homeStatus`.
- **Adds:**
  - `onOpenEqualizer` and `onOpenFormat`
  - `eqFlat: boolean`
  - `outputSampleRate: number`

`app/index.tsx` wires them up. The firewall override moves out of
`MixerHome` into `homeStatus`.

Removed from the home screen:

- the status pill
- `RouteRow`, `RouteDivider` and `SectionLabel`
- the lamp block
- the Sound settings button
- the Disconnect button

## Equalizer (`app/equalizer.tsx`)

- **Header:** the native header stays, titled `EQUALIZER`. The back label is
  "Mixer", taken from the index screen's title.
- **Reset:**
  - It keeps today's disabled rule: all bands at 0, or controls disabled.
  - It keeps the platform split from `shay/dazzling-chebyshev-dd737a`: a
    `Stack.Toolbar` button on iOS, and a `headerRight` caption (`eq-reset`) on
    Android, where toolbar buttons only draw icons.
- **Body:** a `Faceplate` with a non-scrolling `View` and 16 pt padding.
  1. **Curve readout:** a `LedWindow` with a silkscreen `CURVE` and `LedText`
     reading `FLAT` or `CUSTOM` (`eq-curve`).
  2. **Five equal columns,** each holding, top to bottom:
     - **dB readout:** an LED reading in a small `LedWindow`
       (`eq-band-{i}-readout`), for example `+3.5`, `-12` or `0`.
     - **Fader:** a vertical `Fader` (`eq-band-{i}`).
       - −12 to 12 in 0.5 dB steps, with the origin at 0.
       - Accessibility step 1 dB, using today's spoken band names ("Bass, 60
         hertz").
       - Values are spoken as "plus 3.5 decibels", "0 decibels" or "minus 2
         decibels".
       - Disabled when `controlsEnabled` is false.
     - **Frequency:** `60`, `250`, `1K`, `4K` or `12K` in mono.
     - **Name:** Bass, Warmth, Mid, Pres or Air in sans at 12 pt.
  3. **Scale:** `+12`, `+6`, `0`, `−6` and `−12`, printed left of the first
     fader. A 1 pt `grooveLight` 0 dB line runs across all five slots.
  4. **Footnote:** "Drag up to boost, down to cut. 0 dB leaves the sound
     unchanged."
- **Track length:** whatever height is left after the readouts and labels,
  clamped to 140–320 pt.
- **Edge clearance:** the scale column keeps the first fader clear of iOS's
  back-swipe edge.

## Pairing (`src/pairing-home.tsx`)

- **Background:** the screen sits on a `Faceplate`, and every `Card` is
  removed.
- **Discovered desktops:** rows separated by `Groove`s. The empty-discovery
  message is plain text.
- **Manual setup:** the host field sits in a `LedWindow`, with cream Plex Mono
  text.
- **PIN field:** already an LED well. It becomes a `LedWindow`.
- **Connect:** a `PanelKey` with `lit`. It keeps the `lit` and `canPair`
  rules and the spinner.
- **Unchanged:** the scan and manual-setup links, all test IDs, and the
  amber cursor colors from `shay/dazzling-chebyshev-dd737a`.

`card.tsx` and `icon-tile.tsx` are deleted if nothing else uses them.

## Settings forms and navigation

- **Audio format and Connection:**
  - A `Faceplate` sits behind the `Host`.
  - On iOS the form hides its scroll background and drops its background
    color, and rows use `listRowBackground(faceRow)`.
  - On Android the containers are transparent where Compose allows it.
    Anywhere Compose paints its own color, the form falls back to flat
    `faceBottom`, recorded as a known gap.
- **Sound screen:** `app/sound.tsx` and its `stackScreens` entry are deleted.
- **Back labels:** Equalizer and Audio format drop their `"Sound"` back
  titles, so the back label reads "Mixer".
- **Restoring screen:** it sits on a `Faceplate`.

## Error handling

- **Write failures:** unchanged. The session reports them, and the fader lets
  go through `useFaderDraft`.
- **Font load failure:** unchanged. `FontGate` falls through and LED text uses
  system fonts.
- **Gradient failure:** the PNG fallback described under the finish.
- **Unknown sample rate:** the `--.-K` corner.

## Accessibility

- Every touch target is at least 44 pt.
- Faders are adjustable in steps.
- The LED window is read as one summary.
- Keys are buttons that report their disabled state, and the EQ key reports
  Flat or Custom.
- Decorative layers are hidden from screen readers: the grain, grooves, ghost
  segments, scale ticks and the 0 dB line.
- LED text and key labels cap at 1.3× text size. Body text scales normally,
  and the keys wrap 2 × 2 at large sizes.
- No animations are added.

## Testing

TDD: every behavior gets a test that fails first.

**Unit tests (Jest):**

- **Theme:** the new and changed tokens have the values in the tables.
- **`useFaderDraft`:** every case ported from `native-fader.test.tsx`.
- **`Fader`:**
  - Relative drag through responder events, with snapping and clamping.
  - A touch alone never changes the value.
  - A disabled fader ignores touches.
  - Increment and decrement steps.
  - `accessibilityValue.text`.
  - Termination requests are refused.
- **`LedText`:** ASCII renders in DSEG14, uppercased. Non-ASCII renders in Plex
  Mono.
- **`homeStatus`:** every row of the status table.
- **Format corner:** 44100, 48000, 96000 and 0.
- **Mixer home:**
  - The window shows source, speaker, status word and format, and reads as
    one summary.
  - Each key calls its handler.
  - EQ and FORMAT are disabled when `soundDisabled` is true.
  - The EQ indicator shows when any band is nonzero.
  - The status pill, the Sound button and the Disconnect button are gone.
  - Messages keep their test IDs and have no box.
  - The CD transport works as before.
  - The layout test IDs stay.
- **Equalizer:**
  - Five faders and their readouts.
  - The curve readout shows FLAT or CUSTOM.
  - The Reset rule on both platforms.
  - An increment calls `changeBand` with the stepped value.
- **Pairing:** no `Card`s, and the existing pairing tests pass unchanged.
- **Navigation:** there is no `sound` route, and no back title reads "Sound".

**Existing tests that change only their path:**

| Old | New |
| --- | --- |
| `sound-settings-button` → `equalizer-link` | `eq-key` |
| `sound-settings-button` → `audio-format-link` | `format-key` |
| `source-row`, `output-row` | `source-key`, `speaker-key` |
| `status-pill` text | `led-status` word, `led-window` summary |
| `live-dot` | `live-lamp` |
| `disconnect-button` | Connection's `disconnect-menu-button` |
| Native slider `onValueChange` on `volume-slider-native` and `eq-band-{i}-native` | `Fader` accessibility actions or responder events |

**Gates:** `pnpm test:mobile`, `pnpm --filter mobile typecheck`,
`pnpm check:ci` and `pnpm test:native`.

**Device pass:**

- On the iOS 27 simulator against the mock core:
  - Drag the volume and each EQ band, and confirm each value with
    `on-air status --json`.
  - Reset the EQ.
  - Adjust a fader with VoiceOver.
  - Tap every key, pair, and try the largest text size.
- On the Android emulator: the same flows, plus checking that a drag inside
  the home `ScrollView` doesn't scroll, the gradients and grain, and the form
  backgrounds.
- Refresh `docs/screenshots/mobile.png` for the README.

## Overlap with unmerged work

These branches change files this redesign rewrites. They need to be in the
base before the plan runs, or be merged into this branch with the owner's OK.

- **`shay/dazzling-chebyshev-dd737a`** (Android EQ Reset):
  - It touches `app/equalizer.tsx`, `app/sound.tsx`, `src/pairing-home.tsx`
    and `src/settings-form.tsx`.
  - Its Android header Reset and `rowColors` carry forward. Its `sound.tsx`
    edit goes away with the screen.
- **`shay/wizardly-chebyshev-6d3314`** (Android verify fixes): touches
  `app/sound.tsx`, `src/pairing-home.tsx` and `src/settings-form.tsx`.
- **`shay/elastic-edison-283413`** (Android dark-only): touches
  `app/sound.tsx`, `src/pairing-home.tsx`, `src/settings-form.tsx` and the
  Android project.
- **`shay/sweet-bell-f8e85a`** (keyboard clearance for Connect): touches
  `src/pairing-home.tsx`.
