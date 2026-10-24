# iPhone Duo: the app's controls in the system bars

Date: 2026-10-24
Status: direction chosen by the owner in conversation, pending spec review
Builds on: the iPhone Duo work (`2026-10-22-mobile-iphone-duo-design.md`, on `main` as 1.0.0) and the faceplate pickers (`2026-10-23-mobile-faceplate-pickers-design.md`, PR #35)
Supersedes: the pickers spec's decision to keep Expo UI's native `BottomSheet` ("Approach 1")
Branch: `feat/mobile-faceplate-pickers` (PR #35), brought up to date with `main` first

## Goal

On the folded iPhone Duo, iOS moves navigation bars, toolbars and tab bars
into a vertical strip on the side of the outer display. The Back button
already moves there. The app's other top-trailing controls do not: they sit
in the content next to the strip.

| Control | Today | In the strip when folded? |
| --- | --- | --- |
| Back (Equalizer, Audio format, Connection) | native header | yes |
| Equalizer **Reset** | native toolbar item, title only | no: Apple skips items with a title and no icon |
| Mixer **"…"** (More options) | custom key in the content | no: not a bar item |
| Picker **Done** (PR #35) | custom key in the content | no: not a bar item, and the sheets have no bar |

This change makes every one of them a system bar item, so the folded Duo
shows them in the strip, as Apple recommends.

Success means:

- On the folded Duo, Reset, More options, and the pickers' Done/Cancel
  appear in the vertical strip. Close or Back sits at the top of the strip,
  then the screen's actions.
- On every other iPhone, and the unfolded Duo, the same items sit in the
  horizontal bar where the controls are today.
- Android, which has no vertical strip, keeps today's controls.
- Behaviour is unchanged: the same session calls, the same close-on-success,
  the same routing from an unpaired speaker to pairing.

## Apple's guidance this follows

From "Preparing your app for iPhone Duo" (Apple developer documentation):

- "iPhone Duo presents navigation bars, toolbars, and tab bars together
  vertically on the side of the display in some poses."
- Use the bar support that navigation containers provide, not custom bars.
- "If your item has a title and doesn't have an icon, the system doesn't
  present it vertically." Give items both an icon and a title.
- "Reserve the top for primary navigation controls, like Back or Close,
  followed by prominent actions, such as Done."
- "On the outer display, the system presents bars vertically for sheets by
  default."

## Decisions

- **The owner chose, in conversation:**
  - System bars everywhere (over "system bars, keep sheets" and "only fix
    Reset").
  - Merge PR #35 only after this lands on it.
- **Pickers become routes.** The faceplate content from PR #35 (channel
  strips, LED windows, the LED code field) stays. What changes is the
  container: each picker is an expo-router route presented as a native form
  sheet, with a native header and toolbar. The pickers spec rejected routes
  to avoid navigation churn. That churn is now the cost of putting Done in the
  system bar.
- **iOS only for the bars.** Android has no vertical strip. Its form sheets
  keep the in-content caption and Done row, and its mixer keeps the "…" key.

## Design

### Equalizer Reset

- The iOS toolbar button gets an SF Symbol and keeps its title:
  `icon="arrow.counterclockwise"`, title "Reset", accessibility label
  "Reset equalizer".
- In a horizontal bar iOS prefers the icon, so the header shows the arrow.
  Folded, it moves into the strip. Disabled while the curve is flat, as now.
- Android's header caption is unchanged.

### Mixer "More options"

- On iOS the mixer route shows the native header: transparent, no title,
  with a right toolbar button. It has `icon="ellipsis"`, title "More options",
  and opens `/connection`, as the key does today.
- The in-content round "…" key is removed on iOS. The ONAIR sign stays where
  it is, and the bar's button sits in the same band at the trailing edge.
- Folded, the button moves into the strip. On wide windows (unfolded Duo,
  iPad) it stays in the horizontal bar over the mixer pane.
- Android keeps the in-content key.

### Pickers as form-sheet routes

- **Routes:** `app/source.tsx`, `app/speaker.tsx` and `app/pair-speaker.tsx`
  (params `transport`, `id`, `name`), inside the paired-only
  `Stack.Protected`.
- **Presentation:** `presentation: "formSheet"` with the grabber visible.
  - The source and speaker lists open at a medium detent and drag to full;
    their strips scroll.
  - The pair sheet fits its content.
  - Exact detents are set in the device pass.
- **iOS header:**
  - The faceplate header (opaque, `faceTop`), titled with the uppercase
    caption: SOURCE, SPEAKER, PAIR SPEAKER.
  - Source and Speaker get a right toolbar button: `icon="checkmark"`, title
    "Done", closing the sheet.
  - Pair speaker gets a left toolbar button: `icon="xmark"`, title "Cancel".
    "Pair and connect" stays the lit key in the content: it's the sheet's one
    action, next to the code it submits.
- **Android:** the form sheet has no native header. The content shows
  today's caption and Done row; Pair keeps its Cancel and Pair keys.
- **Flow:**
  - The SOURCE and SPEAKER keys push `/source` and `/speaker`.
  - A successful pick, Done and Cancel go back.
  - An unpaired speaker that needs pairing replaces `/speaker` with
    `/pair-speaker` and its params, as the sheet hands over today.
  - `app/index.tsx` loses its sheet state.
- **Content:** `SourceSheet`, `OutputSheet` and `DevicePairSheet` keep their
  faceplate content and tests' testIDs. `SheetPanel` becomes the route body
  (faceplate, padding, and the Android-only caption row). Expo UI's
  `BottomSheet`, `RNHostView` and `sheetProps` leave the pickers.

## Testing

Tests are written first. Each finding from the device pass is a failing
test, then a fix.

- **The router mock** (`__mocks__/expo-router.js`) learns the three routes,
  query params, `useLocalSearchParams`, `router.replace` and form-sheet
  options.
- **Reset:** the toolbar button has the icon, title and label.
- **More options:**
  - On iOS, the mixer's toolbar button has the icon and title, opens
    `/connection`, and the in-content key is gone.
  - On Android the key remains.
- **Pickers:**
  - The routes' options: form sheet, the grabber, iOS toolbar buttons with
    icon and title.
  - Done and Cancel go back; a successful pick goes back.
  - Pair routing replaces the speaker route with the right params.
  - On Android, the in-content caption and Done row.
  - The existing sheet tests keep passing on the new containers.
- **Device pass:**
  - **Folded Duo:** Reset, More options, Done and Cancel are in the strip,
    with Back/Close at the top.
  - **iPhone 17 Pro:** the same items in the horizontal bar.
  - **Unfolded/wide:** two panes, with "…" in the horizontal bar.
  - **Android:** unchanged controls.
  - Default and largest text throughout.

## Out of scope

- The release. 1.0.0 is on TestFlight; whether this ships as 1.0.1 is a
  separate call.
- The two-pane check on a real unfolded Duo (still blocked on the simulator).
- The pickers spec's deferred minor items.

## Risks

- **Form-sheet bars on the folded Duo.** Apple says sheets present bars
  vertically by default. react-native-screens hosts form-sheet headers in a
  UIKit navigation controller, which should qualify. The first task proves
  it on the folded Duo before the pickers move. If it fails, the fallback is
  Expo UI sheets with no in-content Done (the second option offered), and the
  owner is told.
- **The transparent mixer header.** iOS 26+ may draw a scroll-edge effect
  over the faceplate under a transparent bar. If it shows, it is turned off
  for the mixer.
- **Android form sheets.** A nested scroll inside a react-native-screens
  form sheet may need its detents or `ScrollView` tuned on the emulator.
