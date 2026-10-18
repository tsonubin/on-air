# Mobile retro theme

Date: 2026-10-18
Status: approved in conversation, pending spec review

## Goal

Make the phone remote (`apps/mobile`) look like a sibling of the desktop tray
app (`apps/desktop`): warm brown-black faceplates, cream ink, amber values and
controls, a red ONAIR sign and live lamp, LED readouts, and IBM Plex type.
Same features, flows and layouts; only the skin changes.

## Decisions

- **Depth: retro skin over native controls.** Native SwiftUI / Material
  controls stay (sliders, forms, pickers, sheets), as chosen in the September
  redesign (`docs/history/2026-09-mobile-redesign-qa.md`). They take the new
  palette through tint, background and font modifiers. No custom-drawn faders
  or knobs.
- **Palette: the desktop's exact values** (palette A). Hex values come from
  `apps/desktop/src/App.css` so the two apps share one look.
- **Fonts: loaded at runtime** with `expo-font`. No changes to the checked-in
  `ios/` or `android/` projects.

## Color tokens

`src/theme.ts` keeps its token names where they exist and takes the desktop
values. New tokens are added for amber, LED and sign states.

| Mobile token | Desktop token | Value |
| --- | --- | --- |
| `background` | well | `#0a0908` |
| `surface` | face | `#1a1814` |
| `surfaceRaised` (new) | face-2 | `#221f1a` |
| `surfacePressed` | face-hover | `#2a2218` |
| `iconSurface` | face-raised | `#2a261f` |
| `border` | line | `#3a342a` |
| `borderStrong` (new) | line-strong | `#4a4338` |
| `separator` | line-soft | `#2e2a24` |
| `label` | ink | `#f3ead4` |
| `secondaryLabel` | steel | `#c4b89a` |
| `tertiaryLabel` | steel-dim | `#9a8f78` |
| `accent` (live red) | live | `#ff3b2a` |
| `liveDim` (new) | live-dim | `#6a2a22` |
| `amber` (new) | amber | `#e0a24b` |
| `amberBright` (new) | amber-bright | `#f0c56a` |
| `key` (new) | key | `#5c4a32` |
| `onAmber` (new) | face-deep | `#141210` |
| `ledWell` (new) | led-well | `#070605` |
| `ledGhost` (new) | led-ghost | `#1c1812` |
| `signLitText`, `signLitWell`, `signLitLine` (new) | live, sign-live-mid, sign-live-line | `#ff3b2a`, `#1a0605`, `#8a3a30` |
| `signOffText`, `signOffWell` (new) | sign-ok, sign-mid | `#5a1814`, `#0e0c0a` |
| `errorSurface` | danger-bg | `#3a1410` |
| `onError` | danger-fg | `#f0b4ac` |
| `errorBorder` | live-dim | `#6a2a22` |
| `disabled` | steel-dim at reduced contrast | `#5b5246` |

`onAccent` stays white. `selectedSurface` takes the desktop `live-deep`
(`#3a221c`) for the active route row. `theme.seedColor` becomes `amber`.

### Color roles

- **Red means live**, and only live: the lit ONAIR sign, the live lamp, the
  `LIVE` status, and the dot on the active route row.
- **Amber means values and controls**: native slider, switch and picker tint,
  checkmarks, LED readouts, link-style buttons ("Search again"), the source to
  speaker arrow, and the Connect key.
- **Cream on brown-black** for everything else.

## Typography

Fonts are copied as TTF (React Native cannot load woff2) from the same
upstream families the desktop vendors, under the OFL, into
`apps/mobile/assets/fonts/` with `OFL.txt`:

- `Anton-Regular.ttf`
- `DSEG14ClassicMini-Regular.ttf` (keshikan/DSEG)
- `IBMPlexMono-Regular.ttf`, `IBMPlexMono-Medium.ttf`
- `IBMPlexSans-Regular.ttf`, `IBMPlexSans-Medium.ttf`, `IBMPlexSans-SemiBold.ttf`

Static weights, not the variable Plex Sans build, so each weight has a
PostScript name. Fonts register under their PostScript names (the keys passed
to `useFonts`), so the same name works for React Native `fontFamily` and for
SwiftUI `font({ family })`.

`theme.ts` gains a `type` table of presets. Screens use presets instead of
inline `fontSize` / `fontWeight`:

| Preset | Font | Use |
| --- | --- | --- |
| `sign` | Anton, 18 pt, wide tracking | ONAIR sign |
| `led` | DSEG14 Classic Mini | volume, PIN and CD track readouts |
| `label` | Plex Mono Medium 11 pt, uppercase, tracking 0.16 em | section labels, status pill, key captions |
| `title` | Plex Sans SemiBold | headlines, row titles |
| `body` | Plex Sans Regular | copy, subtitles |

Text keeps Dynamic Type / font scaling (no `allowFontScaling={false}`), except
the sign and LED digits, which cap their multiplier so the layout holds.

### Loading

`apps/mobile/package.json` declares `expo-font` (`~57.0.4`, already linked
through `expo`, so no native change). `app/_layout.tsx` calls `useFonts` and
renders a plain `background`-colored view until the fonts load or fail; local
assets load in milliseconds, so no splash-screen package is added. On failure
the app renders with system fonts; presets fall back to the platform default
family.

## Screens

### Home (`src/mixer-home.tsx`)

- **Header:** a small ONAIR sign (Anton 18 pt, 1 pt border, 4 pt radius,
  padding 1 × 8 pt) on the same row and height as the status pill and the
  more button. Lit (red text with a soft red glow, `signLit*`) while live;
  unlit (`signOff*`) otherwise. Its accessible name stays "on-air" with the
  header role. The status pill becomes `● LIVE` / `IDLE` in the `label`
  preset. The more button is a 36 pt round faceplate button with a line
  border, keeping a 46 pt touch target through `hitSlop`.
- **Section labels** ("Now streaming", "Playback"): `label` preset in
  `tertiaryLabel`.
- **Route card:** faceplate (`surface`, `border`, 8 pt radius, 1 px top sheen
  highlight); icon tiles on `iconSurface`; titles `title`, subtitles `body` in
  `secondaryLabel`; amber arrow between rows; a red dot after the active
  speaker's name while live.
- **Live state:** red lamp with a glow while live, `liveDim` when idle.
- **Volume card:** `VOLUME` label and an amber LED readout in an `ledWell`
  recess with `ledGhost` "888" behind it; the native slider tinted amber.
- **CD deck:** LED track counter; transport buttons as faceplate keys.
- **Sound settings row:** faceplate row with a `tertiaryLabel` chevron.
- **Disconnect remote:** `label` preset in `onError`.

### Pairing (`src/pairing-home.tsx`)

- Unlit ONAIR sign in place of the "on-air" title.
- "Pair your Mac" eyebrow in the `label` preset, amber.
- Headline `title`, body `body` in `secondaryLabel`.
- Discovered-Mac card: `surfaceRaised` with a `key` border when selected, and
  an amber check.
- "Search again" amber, "Set up manually" `secondaryLabel`.
- PIN field as an LED input: amber DSEG14 digits in an `ledWell` with ghost
  `888888` replacing the `000000` placeholder. The field keeps its keyboard
  type, focus and submit behavior.
- Connect as an amber key (`amber` background, `onAmber` text, `label` preset
  "CONNECT"); disabled as a `surface` key with `tertiaryLabel` text.

### Shared pieces (`src/ui/`)

`Card` takes the faceplate style. `Notice` and `FirewallNotice` use the
danger pair. `IconTile` and `NativeIcon` take the new surfaces and colors.

### Native screens and sheets

- **Navigation theme** (`app/_layout.tsx`): a theme derived from
  `DarkTheme` with `background`, `card: surface`, `text: label`,
  `border: separator` and `primary: amber`; header titles in Plex Mono
  uppercase; back button tint amber.
- **Forms** (`src/settings-form.tsx`, `app/sound.tsx`, `app/equalizer.tsx`,
  `app/audio-format.tsx`, `app/connection.tsx`): on iOS,
  `scrollContentBackground("hidden")` with a `background` color modifier,
  `listRowBackground(surface)` on rows, `tint(amber)`, and
  `font({ family, textStyle })` on text so Dynamic Type still applies.
  Section titles in Plex Mono uppercase. EQ band values in amber Plex Mono.
- **Bottom sheets** (`src/sheets/sheet-props.ts`): `presentationBackground`
  set to `surface`, `tint(amber)`, dark scheme.
- **Both platforms:** every Expo UI `Host` gets `seedColor={colors.amber}`.
  Expo UI applies it as the SwiftUI tint on iOS and generates a Material 3
  palette from it on Android, so sliders, switches and pickers go amber on
  both without per-control colors. Compose text in native forms on Android
  keeps the system font, because runtime-loaded fonts are only guaranteed in
  React Native text.

## Out of scope

- Custom-drawn faders, knobs or meters.
- Layout, navigation, copy or behavior changes beyond the PIN placeholder and
  the header title becoming the sign.
- Desktop or TUI changes.
- Native project changes in `ios/` or `android/`.

## Testing

- Jest: existing suites pass. Add an `expo-font` mock (fonts ready by
  default). New tests: the root layout renders its children
  once fonts load, and also when font loading returns an error; the header
  exposes the accessible name "on-air" and reflects live versus idle.
- `pnpm typecheck`, `pnpm check:ci` and `pnpm test:mobile` pass.
- Visual check on the iOS 27 simulator against the mock core: pairing, home
  live and idle, Sound, Equalizer, Audio format, Connection, and the source
  and speaker sheets. This needs the UIScene fix from branch
  `shay/interesting-poitras-096ac1`; merging it is the user's call.
- Visual check on an Android emulator if one is available locally.
