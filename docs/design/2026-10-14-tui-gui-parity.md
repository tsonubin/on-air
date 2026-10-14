# on-air — TUI redesign: GUI parity, console layout, mouse

**Date:** 2026-10-14
**Status:** Implemented. Design record written before the code; where they disagree, the code is authoritative.

Companion inventory: a read of `apps/desktop/src` (React), its Tauri shell,
`apps/tui/src` and the core routes in `packages/core/src/lib.rs`. The parity
gaps it found are listed under "Why".

## Why

On Omarchy the `on-air` terminal UI is the main app, and the Tauri desktop is
for macOS and Windows. The TUI should do everything the desktop GUI does and
look good doing it. Today it falls short in these ways:

- The header shows "◌ starting" when the active output has **failed**
  (`apps/tui/src/view.rs`: every non-live state counts as starting).
- Input names are raw ALSA/PipeWire strings. The GUI shows friendly,
  deduplicated names (`apps/desktop/src/ui/prettyInput.ts`, `inputLabels.ts`).
- Destination rows show only "● live". There is no per-row
  connecting/failed state and no stereo-pair badge.
- There is no empty-state help, no status view (version, port, AirPlay
  mode, every LAN address), and no explanatory panel when the monitor is off.
- Volume moves only in steps of 5 and EQ in steps of 1 dB. The GUI does ±1 and 0.5 dB.
- Mixer writes each spawn a task, so they can land out of order. After a
  sample-rate change the other rate's options are never re-read.
- A second Enter while an output is activating is not blocked.
- The TUI refreshes only on live events. It has no background refresh, and
  every event causes a full reload.

Beyond parity, the user asked for four extras: mouse support, the core's
Bluetooth device list (pair or connect from the TUI), CD seek / go to track /
eject, and a level meter. The core already supports all four; neither UI
uses them yet.

GUI features tied to the desktop shell (tray menu, hide-to-tray, `--hidden`
login launch, the macOS AirPlay picker) are out of scope. On Omarchy their
counterparts are the `on-air bar` module, the menu rows and the systemd unit.

## Goals

- Every GUI feature that does not need the desktop shell works in the TUI.
- Mouse support, a Bluetooth device list in Destination, CD seek / go to
  track / eject, and a live level meter.
- A console layout that reads like the GUI's structure, drawn in the
  terminal's and Omarchy's own colours.
- No new crate dependencies. The pure `update` and `view` stay testable
  without a terminal.

## Non-goals

- The GUI's fixed "broadcast hardware" palette, fonts or animations.
  Omarchy-native look was chosen over hardware look.
- Changes to the core, the HTTP API or the desktop/mobile apps.
- A tabbed or multi-screen TUI. Everything stays on one screen.
- macOS AirPlay picker mode. The TUI keeps ignoring `airplay_mode` except
  for showing it in the status overlay.

## Architecture

Approach chosen: evolve the existing Elm-style structure; don't move to a
component framework. `model.rs` holds state, `update.rs` is a pure
`(model, msg) -> Vec<Cmd>`, `view.rs` is a pure render, `client.rs` does
I/O and `main.rs` runs the loop. That stays. The changes:

- `view.rs` becomes `view/` with one module per area: `header`, `cd`,
  `route` (Source + Destination), `mixer`, `overlay` (help, pair, status,
  track prompt, eject confirm, paused panel), plus `layout` for the size
  rules.
- **Hit map.** While rendering, `view` records a `HitMap`: a list of
  `(Rect, Target)` pairs, where `Target` names a control such as
  `SourceRow(i)`, `DestRow(i)`, `Fader(Volume)`, `Fader(Eq(band))`,
  `Rate(In|Out)`, `CdButton(..)`, `CdProgress`, `MonitorTag`, `StatusTag`
  or `OverlayOutside`. The loop keeps the last frame's map. A mouse event
  becomes `Msg::Mouse { target, column, row, kind }` after a lookup, and
  `update` handles it like a key. `HitMap` is plain data, so it can be tested.
- **Write queue.** A small per-control state machine in the model:
  `Idle`, `InFlight`, or `InFlight + queued value`. `update` decides when to
  send, so the latest-value-wins rule is tested in `update` tests.
- **Redraw throttle.** The loop marks the frame dirty and draws at most
  every 50 ms. Keys and mouse events draw at once.

New module `labels.rs` ports the GUI's input naming (`prettyInput` and
`inputLabels`: friendly names plus numeric suffixes for duplicates). It is
pure, and its tests are ported from the GUI's tests where they exist.

## Screen

Designed for the Omarchy floating window, 875×600 px ≈ 100×30 cells.

```
● ON AIR  Sonos · Living Room        PIN 4821  10.0.0.12  monitor on  login ✓
⏮ ▶ ⏭  03/12  Kind of Blue — So What   ━━━━━━━━━━━──────  02:14 / 09:22   ⏏
┌ Source ─────────────────────────┐┌ Destination ──────────────────────────────┐
│ ● Desktop audio                 ││ Sonos                                     │
│   USB mic                       ││ ● Living Room        ⚇ pair        live   │
│   CD drive                      ││   Kitchen                                 │
│                                 ││ AirPlay                                   │
│                                 ││   Office HomePod             ◌ connecting…│
│                                 ││ Bluetooth                                 │
│                                 ││   WH-1000XM4                    paired    │
│                                 ││   JBL Flip 5                 not paired   │
└─────────────────────────────────┘└───────────────────────────────────────────┘
┌ Mixer ─────────────────────────────────────────────────────────────────────────┐
│ VOL  ┃  LVL ▕▇▇▇▇▇▅▂  ┃   60   250   1k    4k   12k        In   48000 Hz      │
│  72  ┃                    │    │    ╪    │    │           Out  44100 Hz      │
│      ┃                   ─┼─   │    │   ─┼─   │                              │
│                          +1.0 dB                                             │
└────────────────────────────────────────────────────────────────────────────────┘
 Tab pane  Enter select  m monitor  i info  ? help  q quit
```

### Header

- **Lamp** (from `active_output.state`):
  - `○ idle`: dim
  - `◌ connecting`: yellow
  - `● ON AIR`: green, bold
  - `✗ failed`: red

  The failed state fixes the bug above.
- **Middle:** the live transport and device name.
- **Right:**
  - `PIN nnnn` and the first LAN address.
  - `monitor on/off`.
  - `login ✓/✗`.
  - `reconnecting` while the live event connection is down.
  - `✗ core unreachable` when the core cannot be reached.
- **Pending action:** shown as now (`Changing monitor…`).

### CD strip

Only shown when a disc is present:

- Transport buttons, `track/count`, album — title, a one-line progress bar,
  `position / duration`, and an eject button.

### Error and notice lines

As today: red `✗` and yellow `!`, dismissed with Esc.

### Source | Destination

Side by side, about 40/60 when the width is at least 90 columns. Below that,
Source stacks above Destination.

**Source:**
- Labels come from `labels.rs`. `●` marks the active input.
- Empty state: "No capture devices found. Check PipeWire, then press r."

**Destination:**
- Grouped as Sonos / AirPlay / Bluetooth.
- A right-aligned tag shows each row's state:

  | Tag | Meaning |
  |---|---|
  | `live` | the active output is live |
  | `◌ connecting…` | the active output is starting, or a local activation is pending |
  | `◌ pairing…` | a local pairing step is pending |
  | `✗ failed` | the active output failed, or the last local activation of this row failed |
  | `⚇ pair` | `kind = pair` or `member_count ≥ 2`; shown as well as the state tag |
  | `pin` | an AirPlay device that needs a PIN |
  | `paired` / `not paired` | Bluetooth devices that are not live |
  | `unavailable · <info>` | an output the core reports as unavailable |

- Empty group help, one dim line each:
  - Sonos: "None found on this network — press r to rescan"
  - AirPlay: "None found — AirPlay needs OwnTone or pyatv"
  - Bluetooth: "None paired — press b for Bluetooth settings"

### Mixer

- **Volume:** a vertical fader with its value.
- **Level meter:**
  - A horizontal RMS bar (`▏▎▍▌▋▊▉█` eighths) plus a peak tick.
  - The peak holds for 0.5 s, then falls 1.5 dB per 50 ms tick.
  - It reads zero when no level event has arrived for 1 s, when nothing
    is live, or when the monitor is off.
  - Scale: dBFS from −60 to 0.
- **EQ:** five vertical faders (60, 250, 1k, 4k, 12k) from −12 to +12 dB,
  with a centre mark. The focused band shows its dB value underneath.
- **Sample rates:** `In` and `Out` rows.
- **Focus:** inside the Mixer pane, `j/k` moves between volume, the five
  bands, In and Out, as today.

### Monitor off

The Source/Destination area is replaced by a centred panel:

> **Monitor is off.** Capture and streaming are stopped. Discovery and the
> phone remote stay online. Press **m** to turn it on.

The mixer is dimmed. As today, while the monitor is off only
`m`, `a`, `i`, `?`, `Esc` and `q` work.

### Status overlay (`i`)

Shows:
- version and port (`47990`)
- every LAN address as `addr:47990`
- the AirPlay mode
- monitor on/off and whether sleep is inhibited
- login start (systemd unit enabled or not)
- the core base URL

Closed with Esc, `i` again, or a click outside.

### Small terminals

- Width below 90 columns: Source stacks above Destination.
- Height below 24 rows: the EQ falls back to one line per band (today's
  horizontal bars) and the volume fader becomes a horizontal bar.
- Below 40×10: the existing "please resize" message.

### Colours

- Backgrounds stay `Color::Reset`, so terminal transparency shows through.
- The Omarchy accent colour (`theme.rs`, reloaded live) marks focus and
  the active row.
- Green means live, yellow means pending or warning, red means failed or
  error, cyan means the CD strip, and dim means unavailable or locked.
- No fixed palette.

## Interaction

### Keys

Every existing key is unchanged. New keys:

| Key | Action |
|---|---|
| `H` / `L` | Fine adjust the focused mixer control: volume ±1, EQ ±0.5 dB |
| `[` / `]` | CD: seek −10 s / +10 s (clamped to 0..duration) |
| `g` | CD: go to track. Opens a prompt; type digits, Enter to go, Esc to cancel |
| `e` | CD: eject, after a y/n confirm |
| `i` | Status overlay |

`h`/`l` and `+`/`-` keep today's coarse steps (volume ±5, EQ ±1 dB). CD keys
do nothing when no disc is present. The key table in `keys.rs` stays the one
source for the help overlay and the hint line. The hint line adds `i info`.

### Enter on a Destination row

The row's state decides the steps; each step shows in the row tag:

| Row | Steps |
|---|---|
| Ready output | activate (as today) |
| AirPlay needing a PIN | PIN dialog → pair → activate (as today) |
| Bluetooth, not paired | confirm dialog → pair → connect → activate |
| Bluetooth, paired, not connected | connect → activate |
| Unavailable | nothing; a notice repeats the reason |

Only one activation runs at a time. Enter while one is pending shows
"Already connecting <name>…" and sends nothing.

### Mouse

Enabled with crossterm mouse capture. It is turned off again on exit and
inside the panic hook, the same as raw mode.

| Where | Action |
|---|---|
| Source / Destination row | left click: focus the pane, move the cursor, then Select (same as Enter) |
| List | wheel: move the cursor |
| Fader (volume, EQ band) | left click or drag: set the value at that cell; wheel: fine step |
| Rate row | left click: next rate; wheel: next/previous |
| CD buttons | left click: prev / play-pause / next / eject (eject still asks y/n) |
| CD progress bar | left click: seek to that position |
| `monitor on/off` tag | left click: toggle the monitor |
| `PIN` / address tag | left click: open the status overlay |
| Outside an open overlay | left click: close it (same as Esc) |

Fader mapping: the top cell is the maximum and the bottom cell the minimum,
interpolated linearly and rounded to the control's fine step. A drag sends
writes through the write queue, so only the latest value goes out.

The help overlay gains a "Mouse" section and says: "Hold Shift while
dragging to select text." If enabling mouse capture fails, the TUI keeps
working with the keyboard and the help overlay says "mouse off".

## Data and live updates

### Snapshot

The snapshot gains a twelfth part, `GET /api/bluetooth/devices`, loaded in
parallel with the rest through the existing `take_part` (a 503 while paused
is not treated as an error).

The Destination model merges two lists by device id (Bluetooth outputs use
the BlueZ device id):
- Bluetooth entries from `/api/outputs`.
- `BluetoothDeviceInfo { id, name, paired, connected }` from the device list.

If the device list fails, the group shows the outputs only, plus a dim
"Bluetooth list unavailable" line. No error banner.

### Level meter

- `WsEvent::LevelMeter { rms, peak }` arrives about 43 times a second
  (1024 frames at 44.1 kHz).
- `update` stores the latest values and the time they arrived.
- The 50 ms tick applies peak hold and falloff, and the 1 s timeout.

### Ticks and redraws

- New 50 ms tick (`Msg::Frame`): meter decay, redraw if dirty.
- The existing 1 s tick stays for theme reloads and timeouts.
- Redraw rules:
  - Keys and mouse events: draw at once.
  - Everything else: sets `dirty`, drawn on the next `Frame`.

### Mixer writes

Each control (volume, EQ, input rate, output rate) holds a `WriteSlot`:

- **Idle**: a change sends a write and moves to `InFlight`.
- **InFlight**: a change replaces `queued` and sends nothing.
- **On success**:
  - If `queued` is set, send it and stay `InFlight`. Otherwise go back to `Idle`.
  - For a rate, also re-read `/api/sample-rate` so both sides'
    `supported_hz` are current.
- **On failure:** drop `queued`, go back to `Idle`, reload the snapshot so
  the control returns to the core's value, and show the error.

The model's displayed value changes at once (optimistic), as today. All
five EQ bands go in one `PUT /api/eq`, so EQ is one slot.

### Keeping in sync

- CD events patch in place (as today).
- Output, input, device and monitor events schedule a refresh with a 300 ms
  debounce. A burst of events produces one snapshot load.
- `DeviceJoined` adds a dim placeholder row immediately; the refresh
  replaces it. `DeviceLeft` removes the row immediately.
- Background refresh every 15 s, skipped while the connection is not open.
  This matches the GUI's 15 s poll.

## Errors

- Error copy stays in `error_copy.rs`, the same sentences as the GUI plus
  TUI-specific hints.
- **Activation failure:** the row shows `✗ failed`, the error line explains
  why, and Enter on the row retries.
- **Bluetooth pair/connect failure:** the error shows inside the dialog. Esc
  closes the dialog and the row returns to its previous state.
- **Core unreachable:** the panes keep their last snapshot, dimmed. Actions
  that need the core show "on-air-core is not reachable" and send nothing.
- **CD actions without a disc:** the keys do nothing and the strip is hidden.

## Testing

Run under `cargo test -p on-air-tui` with `HTTP_PROXY` set, as on the
user's machine, plus `cargo clippy --all-targets`.

- **`update.rs`:**
  - Each Destination Enter path (the five rows of the table above).
  - A second Enter while pending.
  - `WriteSlot` transitions: queue, latest value wins, failure snaps back,
    rate success re-reads the rates.
  - Refresh debounce and skipping the 15 s refresh while disconnected.
  - Meter hold, falloff and timeout.
  - Every new key, including the `g` prompt and the `e` confirm.
  - Every mouse target, given as `Msg::Mouse` with a target.
- **Hit map:** render to a `TestBackend` and check that the cells of each
  control map to the right `Target`. Check fader cell-to-value mapping,
  including the endpoints and rounding.
- **`view`** (render to `TestBackend` and check the text):
  - The four lamp states.
  - Every row tag and the pair badge.
  - Empty-state lines.
  - The monitor-off panel and the status overlay.
  - The CD strip, with and without a disc.
  - Layouts at 100×30, 80×24 (stacked) and 60×20 (compact EQ).
- **`labels.rs`:** ported cases from the GUI's naming and dedupe rules.
- **`tests/mock_core.rs`** against the core in mock mode:
  - Loading the Bluetooth list.
  - Pair → connect → activate for the mock unpaired device.
  - CD seek, go to track and eject.
  - Mock mode has input names but no audio pipeline, so it cannot emit
    level events. Instead, a `client.rs` unit test checks that a
    `LevelMeter` frame in the core's wire format decodes to `WsEvent::LevelMeter`.
    Meter behaviour is covered by the `update.rs` tests.
- **Manual:** run the TUI at 100×30 against the real core, then compare
  before/after screenshots.
