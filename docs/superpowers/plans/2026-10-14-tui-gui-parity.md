# TUI GUI-Parity Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the `on-air` TUI into a console layout with every
desktop-GUI feature that doesn't need the desktop shell, plus mouse support,
a Bluetooth device list, CD seek / go to track / eject, and a level meter.

**Architecture:** Keep the Elm-style split: `model.rs` (state), `update.rs`
(pure `update(&mut Model, Msg) -> Vec<Cmd>`), `view` (pure render),
`client.rs` (I/O), `main.rs` (loop). `view.rs` becomes a `view/` module
tree. Rendering returns a `HitMap` that the loop uses to turn mouse events
into `Msg::Mouse`. All timing inside `update` counts 50 ms `Msg::Frame`
ticks, so timers stay pure and testable.

**Tech Stack:** Rust 2021, ratatui 0.30, crossterm 0.29 (event-stream,
mouse capture), tokio, reqwest. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-14-tui-gui-parity-design.md`

## Global Constraints

- No new crate dependencies in `apps/tui/Cargo.toml`.
- The core, the HTTP API, and the desktop/mobile apps are not changed.
- Backgrounds stay `Color::Reset`. Colours: the Omarchy accent from
  `theme.rs` marks focus and the active row; green = live, yellow =
  pending/warning, red = failed/error, cyan = CD, dim = unavailable/locked.
- Target size is 100×30. Stack Source above Destination below 90 columns.
  Use the compact mixer below 24 rows. Keep the "please resize" message
  below 40×10 (`MIN_SIZE`).
- Frame tick is 50 ms. Derived counts:
  - `PEAK_HOLD_FRAMES = 10` (0.5 s)
  - `PEAK_FALL_DB = 1.5` per frame
  - `METER_TIMEOUT_FRAMES = 20` (1 s)
  - `REFRESH_DEBOUNCE_FRAMES = 6` (300 ms)
  - `BACKGROUND_REFRESH_FRAMES = 300` (15 s)
- Meter scale: −60 to 0 dBFS.
- Steps:
  - `VOLUME_STEP = 5` and `EQ_STEP_DB = 1.0` (coarse, unchanged)
  - `VOLUME_FINE_STEP = 1` and `EQ_FINE_STEP_DB = 0.5`
  - CD seek `±10_000` ms, clamped to `0..=duration_ms`
- Exact copy:
  - Monitor-off panel: "Monitor is off." / "Capture and streaming are
    stopped. Discovery and the phone remote stay online." / "Press m to
    turn it on."
  - Empty Sources: "No capture devices found. Check PipeWire, then press r."
  - Empty groups:
    - Sonos: "None found on this network — press r to rescan"
    - AirPlay: "None found — AirPlay needs OwnTone or pyatv"
    - Bluetooth: "None paired — press b for Bluetooth settings"
  - Bluetooth list failure: "Bluetooth list unavailable"
  - Busy: "Already connecting {name}…"
  - Core down: "on-air-core is not reachable"
  - Help mouse note: "Hold Shift while dragging to select text."
  - Help when mouse capture fails: "mouse off"
- Spec clarification: the GUI's `unavailable · info` tag exists only for
  macOS AirPlay picker mode, which is out of scope. The TUI has no
  `unavailable` tag.
- Every test runs under `cargo test -p on-air-tui` with
  `HTTP_PROXY=http://127.0.0.1:7897` set; clippy must be clean with
  `cargo clippy -p on-air-tui --all-targets -- -D warnings`.

## Review Focus

1. A mouse event on no target, or after a resize before the next draw,
   must do nothing and not panic. Task 11 adds
   `mouse_outside_every_target_is_ignored`.
2. A queued mixer write followed by a failed in-flight write must drop the
   queue and reload, never send the stale queued value. Task 6 adds
   `failure_drops_the_queued_value`.
3. The disc is ejected or removed while the `g` prompt or the eject
   confirm is open: close the overlay and send nothing. Task 8 adds
   `prompt_closes_when_the_disc_goes`.
4. Level events keep arriving after the monitor is turned off, or with
   nothing live: the meter shows zero. Task 5 adds
   `meter_reads_zero_when_monitor_off_or_idle`.
5. An activation's row disappears (`DeviceLeft`) while it is pending: the
   pending state clears when its `Done` arrives, and no stale `✗ failed`
   is left on another row. Task 7 adds `done_for_a_vanished_row_clears_state`.

---

### Task 1: Input labels

**Files:**
- Create: `apps/tui/src/labels.rs`
- Modify: `apps/tui/src/lib.rs` (add `pub mod labels;`), `apps/tui/src/view.rs` (`sources_pane` shows labels)

**Interfaces:**
- Produces:
  - `pub fn pretty_input(name: &str) -> String`
  - `pub struct InputLabel { pub name: String, pub label: String }`
  - `pub fn input_labels(names: &[String]) -> Vec<InputLabel>`

- [ ] **Step 1: Write failing tests in `labels.rs`**

  Port the GUI's rules exactly (`apps/desktop/src/ui/prettyInput.ts`,
  `inputLabels.ts`):

  ```rust
  #[test] fn shortens_common_capture_names() {
      assert_eq!(pretty_input("Discard all samples (playback)"), "Null device");
      assert_eq!(pretty_input("alsa_output.pci.analog-stereo.monitor"), "Analog monitor");
      assert_eq!(pretty_input("alsa_output.usb.monitor"), "System monitor");
      assert_eq!(pretty_input("PipeWire Sound Server"), "PipeWire");
      assert_eq!(pretty_input("Default ALSA Output"), "Default");
      assert_eq!(pretty_input("HDA Intel PCH, CS4208 Analog"), "Built-in analog");
      assert_eq!(pretty_input("HDA Intel HDMI, HDMI 0"), "HDMI HDMI 0");
      assert_eq!(pretty_input("Audio CD"), "Audio CD");
      assert_eq!(pretty_input("USB mic"), "USB mic");
  }
  #[test] fn dedupes_names_and_suffixes_label_clashes() {
      let names: Vec<String> = ["a.monitor", "a.monitor", "b.monitor", "USB mic"].map(String::from).into();
      let labels: Vec<_> = input_labels(&names).into_iter().map(|l| (l.name, l.label)).collect();
      assert_eq!(labels, vec![
          ("a.monitor".into(), "System monitor".into()),
          ("b.monitor".into(), "System monitor (2)".into()),
          ("USB mic".into(), "USB mic".into()),
      ]);
  }
  ```

- [ ] **Step 2:** Run `cargo test -p on-air-tui labels`. Expected: FAIL, the functions are not defined.

- [ ] **Step 3: Implement the three items, following the TS rule order exactly.**
  `pretty_input` checks "Audio CD" first. In `input_labels`, the label
  base counts start at 1, and the second clash gets `" (2)"`.

- [ ] **Step 4: Make `sources_pane` render labels.**
  Render `input_labels(&model.snapshot.inputs)` labels. `●` is set by
  matching `name` against `active_input`. Selecting a row still sends the
  raw `name`. Update the source rows in `view.rs` tests: `Mock Monitor`
  stays as is, since it matches no rule.

- [ ] **Step 5:** Run `cargo test -p on-air-tui`. Expected: all pass.

- [ ] **Step 6: Commit**

  ```bash
  git commit -am "feat(tui): friendly input labels ported from the GUI"
  ```

### Task 2: Bluetooth list in the snapshot, and the Destination row model

**Files:**
- Modify:
  - `apps/tui/src/model.rs`
  - `apps/tui/src/client.rs` (`load_snapshot`)
  - `apps/tui/src/update.rs` (`clamp_selection`, `select`, `move_selection` use the new rows)
  - `apps/tui/src/view.rs` (`outputs_pane` uses `dest_rows`)
- Test: `model.rs` tests module, `tests/mock_core.rs`

**Interfaces:**
- Produces:
  - `Snapshot.bluetooth: Option<Vec<BluetoothDeviceInfo>>`
    (`None` = the list failed to load)
  - `pub enum DestState { Live, Connecting, Pairing, Failed, Pin, Connected, Paired, NotPaired, Idle }`
  - `pub struct DestEntry { pub transport: &'static str, pub id: String, pub name: String, pub pair_badge: bool, pub state: DestState, pub placeholder: bool }`
  - `pub enum DestRow { Header(&'static str), Entry(DestEntry), Empty(&'static str) }`
  - `impl Model { pub fn dest_rows(&self) -> Vec<DestRow>; pub fn selected_dest(&self) -> Option<&DestEntry> }`
    Cursor rows are `Entry` rows only.
  - `pub struct Activation { pub transport: String, pub id: String, pub name: String, pub step: ActivationStep }`
  - `pub enum ActivationStep { Pairing, Connecting, Activating }`
  - `Model.activation: Option<Activation>`
  - `Model.failed_output: Option<(String, String)>` (transport, id)
  - `Model.placeholders: Vec<(String, String, String)>` (transport, id, name), used by Task 10
  - `Model::output_rows` / `OutputRow` / `selected_output` are removed;
    callers use `dest_rows` / `selected_dest`. `output_index` now indexes
    `dest_rows()`.

- [ ] **Step 1: Write failing tests in `model.rs`**

  - `dest_state_precedence`:
    - The active output with `OutputPhase::Failed` → `Failed`; with
      `Starting` → `Connecting`; with `Live` → `Live`.
    - `activation` with step `Pairing` → `Pairing`; with step
      `Connecting` or `Activating` → `Connecting`.
    - `failed_output` match → `Failed`.
    - AirPlay with `needs_pair && !paired` → `Pin`.
    - Bluetooth device `connected` → `Connected`; paired only → `Paired`;
      not paired → `NotPaired`.
    - Otherwise → `Idle`.
    - The order of precedence is exactly the order listed.
  - `bluetooth_list_merges_by_id`:
    - The outputs hold `bt-speaker`.
    - The list holds `bt-speaker` (connected) and `bt-new` (not paired,
      not in outputs).
    - Under the Bluetooth header: exactly two entries, `bt-speaker` once
      with state `Connected`, and `bt-new` with `NotPaired`.
  - `bluetooth_list_failure_adds_unavailable_line`: with
    `snapshot.bluetooth = None`, the Bluetooth group ends with
    `DestRow::Empty("Bluetooth list unavailable")`.
  - `empty_groups_get_help_lines`: no outputs → each header is followed by
    its `Empty(..)` line, using the exact copy from Global Constraints.
    The Bluetooth group uses the "None paired" line only when the list
    loaded and is empty.
  - `pair_badge_for_pairs`: `kind == "pair"` or `member_count >= 2` → `pair_badge`.

- [ ] **Step 2:** Run `cargo test -p on-air-tui model`. Expected: FAIL.

- [ ] **Step 3: Implement `dest_rows` and its types.**
  Go through `TRANSPORTS` in order. Bluetooth entries come from the
  outputs first, then list-only devices in list order.

- [ ] **Step 4: Load the Bluetooth list in `load_snapshot`.**
  Add `self.bluetooth_devices()` to the `tokio::join!`. Store
  `Ok(v) → Some(v)` and `Err(_) → None`. Never push to `failures`.

- [ ] **Step 5: Port the callers to `dest_rows` / `selected_dest`.**
  Port `update.rs` and `view.rs`. `outputs_pane` renders `Empty` rows dim
  and keeps `"(pair)"` for now; Task 4 replaces the tags. Rewrite the
  existing `update.rs` / `view.rs` tests that built `OutputRow` indices.

- [ ] **Step 6: Add a `mock_core.rs` test.**
  `snapshot_includes_the_bluetooth_list`: `load_snapshot` returns
  `bluetooth: Some(list)` containing `bt-unpaired`.

- [ ] **Step 7:** Run `cargo test -p on-air-tui` and clippy. Expected: pass.

- [ ] **Step 8: Commit**

  ```bash
  git commit -am "feat(tui): Bluetooth device list and per-row destination state"
  ```

### Task 3: Split `view.rs` into `view/`

**Files:**
- Create:
  - `apps/tui/src/view/mod.rs`
  - `apps/tui/src/view/layout.rs`
  - `apps/tui/src/view/header.rs`
  - `apps/tui/src/view/cd.rs`
  - `apps/tui/src/view/route.rs`
  - `apps/tui/src/view/mixer.rs`
  - `apps/tui/src/view/overlay.rs`
  - `apps/tui/src/view/hit.rs`
- Delete: `apps/tui/src/view.rs`

**Interfaces:**
- Produces:
  - `pub fn view(model: &Model, frame: &mut Frame) -> HitMap`
    (callers that ignore it use `let _ =`)
  - `pub struct HitMap(Vec<(Rect, Target)>)` with
    `pub fn push(&mut self, rect: Rect, target: Target)` and
    `pub fn hit(&self, column: u16, row: u16) -> Option<(Target, Rect)>`
    (the last pushed match wins, so overlays pushed later take precedence)
  - `pub enum Target { SourceRow(usize), DestRow(usize), Fader(MixerRow), Rate(MixerRow), CdButton(CdButton), CdProgress, MonitorTag, StatusTag, Overlay, OverlayOutside }`
  - `pub enum CdButton { Prev, PlayPause, Next, Eject }`
  - `view::truncate`, `MIN_SIZE`, and the constants re-exported unchanged

- [ ] **Step 1: Move the code without changing behaviour.**
  Move each function into its module and keep the tests in `mod.rs`.
  `hit.rs` holds `HitMap`, `Target` and its own unit test:
  `hit_prefers_the_last_pushed_rect` (two overlapping rects, the later
  one wins; a miss returns `None`).

- [ ] **Step 2:** Run `cargo test -p on-air-tui`. Expected: same pass count plus 1.

- [ ] **Step 3: Commit**

  ```bash
  git commit -am "refactor(tui): split view into modules and add the hit map"
  ```

### Task 4: Console layout, header lamp, Destination tags, empty states

**Files:**
- Modify:
  - `apps/tui/src/view/mod.rs`
  - `apps/tui/src/view/layout.rs`
  - `apps/tui/src/view/header.rs`
  - `apps/tui/src/view/route.rs`

**Interfaces:**
- Consumes: `Model::dest_rows`, `DestState` (Task 2); `HitMap`, `Target` (Task 3)
- Produces:
  - `layout::Areas { header, cd, error, notice, source, dest, mixer, hints: Rect }`
  - `pub fn areas(model: &Model, area: Rect) -> Areas`
  - `pub const STACK_BELOW_COLUMNS: u16 = 90`
  - `pub const COMPACT_BELOW_ROWS: u16 = 24`

- [ ] **Step 1: Write failing tests (render to `TestBackend`, assert on text).**

  - `lamp_states`:
    - No active output → `○ idle`.
    - `Starting` → `◌ connecting`.
    - `Live` → `● ON AIR` followed by `Sonos · Mock Sonos`.
    - `Failed` → `✗ failed` (this is the bug fix).
  - `header_right_side`: contains `PIN 123456`, `10.0.0.12`, `monitor on`
    and `login ✓` (with `autostart = Some(true)`).
  - `dest_tags`: rows show `live`, `◌ connecting…`, `◌ pairing…`,
    `✗ failed`, `⚇ pair`, `pin`, `connected`, `paired`, `not paired`,
    each set up through the model.
  - `empty_state_lines`: the exact copy appears when there are no outputs
    or no inputs.
  - `layout_100x30_side_by_side`: Source and Destination share a row;
    the Destination width is 55–65% of the total.
  - `layout_80x24_stacks`: the Source block sits above the Destination block.
  - Each test also checks that `HitMap::hit` at a Source row cell gives
    `SourceRow(i)`, and at a Destination entry cell gives `DestRow(i)`
    (with `i` = the index in `dest_rows()`).

- [ ] **Step 2:** Run `cargo test -p on-air-tui view`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - The layout is vertical: header, cd, error, notice, route, mixer
    (height 7, or 4 when compact), hints.
  - Route is horizontal 40/60, or stacked when the width is below 90.
  - Push a hit rect per visible row, and push `MonitorTag` / `StatusTag`
    rects for the header spans (compute their x offsets while building
    the spans).
  - The hint line adds `i info`.

- [ ] **Step 4:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 5: Commit**

  ```bash
  git commit -am "feat(tui): console layout, lamp states and destination tags"
  ```

### Task 5: Mixer strip and level meter

**Files:**
- Modify:
  - `apps/tui/src/model.rs` (add `Meter`)
  - `apps/tui/src/update.rs` (`Msg::Frame`, `LevelMeter`)
  - `apps/tui/src/view/mixer.rs`
  - `apps/tui/src/client.rs` (decode test)

**Interfaces:**
- Produces:
  - `pub struct Meter { pub rms_db: f32, pub peak_db: f32, pub hold: u8, pub quiet: u8 }`
  - `Model.meter: Meter` (default `-60.0`, `-60.0`, `0`, `0`)
  - `Msg::Frame`
  - `pub fn level_db(linear: f32) -> f32` (`20·log10`, clamped to `-60.0..=0.0`; `0.0` maps to `-60.0`)
  - `pub fn fader_value(offset: u16, extent: u16, min: f32, max: f32, step: f32) -> f32`
    in `view/mixer.rs`: offset 0 = top = `max`, offset `extent-1` =
    bottom = `min`, linear in between, rounded to the nearest `step`.
  - `Target::Fader(MixerRow)` rects for Volume and `Eq(0..5)`;
    `Target::Rate(InputRate|OutputRate)` rects

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `level_event_sets_meter`: `LevelMeter { rms: 0.5, peak: 1.0 }` →
    `rms_db ≈ -6.02`, `peak_db == 0.0`, `hold == 10`, `quiet == 0`.
  - `peak_holds_then_falls`:
    - After 10 `Frame`s, `peak_db` is unchanged.
    - On the 11th it drops by `1.5`.
    - A new, higher peak resets the hold.
  - `meter_times_out`: 20 `Frame`s without an event → both values at `-60.0`.
  - `meter_reads_zero_when_monitor_off_or_idle` (Review Focus 4): with
    `service_enabled = false`, or no active output, a level event leaves
    the meter at `-60.0`.

- [ ] **Step 2: Write failing tests in `view/mixer.rs` and `client.rs`.**

  - `fader_value_endpoints_and_rounding`:
    - `fader_value(0, 5, -12.0, 12.0, 0.5) == 12.0`
    - `fader_value(4, 5, -12.0, 12.0, 0.5) == -12.0`
    - `fader_value(2, 5, -12.0, 12.0, 0.5) == 0.0`
    - `fader_value(1, 4, 0.0, 100.0, 1.0) == 67.0`
  - `mixer_renders_faders_meter_rates` at 100×30: contains `VOL`, the
    value `72`, `60`, `250`, `1k`, `4k`, `12k`, `In`, `Out`, and the dB
    value of the focused band. The `HitMap` has `Fader(Volume)` and
    `Fader(Eq(2))` rects whose height is at least 3.
  - `compact_mixer_below_24_rows` at 100×22: today's one-line-per-band
    bars, and a horizontal volume bar.
  - `level_meter_frame_decodes` in `client.rs`:
    `serde_json::from_str::<WsEvent>(r#"{"type":"LevelMeter","rms":0.25,"peak":0.5}"#)`
    gives `WsEvent::LevelMeter { rms: 0.25, peak: 0.5 }`.

- [ ] **Step 3: Implement the meter and the mixer view.**
  - Meter decay runs only on `Msg::Frame`.
  - The meter bar uses eighth blocks `▏▎▍▌▋▊▉█` across its width for
    `rms_db`, plus a `│` tick at `peak_db`.
  - Faders are vertical columns of `┃` (filled) and `│` (empty), with a
    `┼` centre mark for EQ.
  - When `model.locked()`, the mixer renders dim.

- [ ] **Step 4:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 5: Commit**

  ```bash
  git commit -am "feat(tui): vertical mixer, level meter and frame tick"
  ```

### Task 6: Mixer write queue and fine steps

**Files:**
- Modify:
  - `apps/tui/src/model.rs`
  - `apps/tui/src/update.rs`
  - `apps/tui/src/client.rs` (`run_cmd` for the mixer commands)
  - `apps/tui/src/keys.rs` (`H`/`L`)

**Interfaces:**
- Produces:
  - `pub enum MixerControl { Volume, Eq, InputRate, OutputRate }`
  - `pub enum WriteSlot { Idle, InFlight { queued: Option<Cmd> } }`
  - `Model.writes: [WriteSlot; 4]`, indexed by `MixerControl as usize`
  - `Msg::Written { control: MixerControl, result: Result<(), String> }`,
    which `run_cmd` returns for `SetVolume`, `SetEq` and `SetSampleRate`
    in place of `Done { LABEL_MIXER }`
  - `Cmd::LoadRates` → `run_cmd` returns `Msg::Rates(Result<SampleRateResponse, String>)`
  - `Action::FineUp` / `Action::FineDown` (keys `L` / `H`)
  - Constants `VOLUME_FINE_STEP: u8 = 1` and `EQ_FINE_STEP_DB: f32 = 0.5`
  - `fn write(model: &mut Model, control: MixerControl, cmd: Cmd) -> Vec<Cmd>`
    is the single entry point every mixer change goes through, including
    mouse changes from Task 11.

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `first_write_sends_and_marks_in_flight`: `l` on Volume →
    `[SetVolume(55)]`, and the slot is `InFlight { queued: None }`.
  - `latest_value_wins`:
    - Three more `l` presses while in flight send nothing; the queue
      holds `SetVolume(70)`.
    - `Written { Volume, Ok }` → `[SetVolume(70)]`, still in flight.
    - The next `Ok` → `Idle`, and no commands.
  - `failure_drops_the_queued_value` (Review Focus 2): in flight with
    `SetVolume(70)` queued, `Written { Volume, Err("x") }` →
    `[Cmd::Refresh]`. The slot is `Idle`, `error == Some("x")`, and
    `SetVolume(70)` is never returned.
  - `rate_success_reloads_rates`: `Written { InputRate, Ok }` with nothing
    queued → `[Cmd::LoadRates]`. `Msg::Rates(Ok(r))` replaces
    `snapshot.rates`.
  - `fine_steps`:
    - `L` on Volume at 50 → `SetVolume(51)`.
    - `L` on `Eq(1)` at 0.0 → `SetEq` with band 1 at `0.5`.
    - `H` mirrors both.
  - `controls_are_independent`: Volume in flight does not block an EQ write.

- [ ] **Step 2:** Run the tests. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - Route `set_volume`, `set_eq_band` and the rate changes through
    `write`. The optimistic model update stays.
  - Drop `LABEL_MIXER` from `pending`, so mixer writes no longer occupy
    the header's pending label.
  - Map `Ctrl`-free `KeyCode::Char('H')` / `'L'` in `keys.rs`.
  - Add both keys to `KEYS` with help "Fine adjust (volume ±1, EQ ±0.5 dB)",
    and update the key-count assertion.

- [ ] **Step 4:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 5: Commit**

  ```bash
  git commit -am "feat(tui): latest-value mixer writes, fine steps, rate reload"
  ```

### Task 7: Destination Enter flow (Bluetooth pair → connect → activate)

**Files:**
- Modify:
  - `apps/tui/src/update.rs` (`select`, `on_done`)
  - `apps/tui/src/model.rs` (`PairTarget` is unchanged)
- Test: `update.rs`, `tests/mock_core.rs`

**Interfaces:**
- Consumes: `DestEntry`, `DestState`, `Activation`, `ActivationStep`,
  `failed_output` (Task 2)
- Produces: the `LABEL_CONNECT` done handling. While `activation` is set,
  the activating row shows its step.

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `enter_on_ready_output_activates`: `[ActivateOutput]`, and
    `activation` step `Activating`.
  - `enter_on_airplay_pin_opens_pin_modal`: as today; on submit, step `Pairing`.
  - `enter_on_unpaired_bluetooth_confirms_then_pairs_connects_activates`:
    - Enter → `Modal::Pin` with a `Bluetooth` target.
    - Enter again → `[PairBluetooth(id)]`, step `Pairing`.
    - `Done { LABEL_PAIR, Ok }` → `[ConnectBluetooth(id)]`, step `Connecting`.
    - `Done { LABEL_CONNECT, Ok }` → `[ActivateOutput { "bluetooth", id }]`,
      step `Activating`.
    - `Done { LABEL_OUTPUT, Ok }` → `activation == None`, plus `[Refresh]`.
  - `enter_on_paired_bluetooth_connects_first`: a paired, not connected
    row → `[ConnectBluetooth(id)]`, with no modal.
  - `enter_on_connected_bluetooth_activates`.
  - `second_enter_while_busy_is_refused`: with `activation` set, Enter on
    another row → no commands, and
    `notice == Some("Already connecting Mock Sonos…")`.
  - `activation_failure_marks_the_row`: `Done { LABEL_OUTPUT, Err }` →
    `failed_output == Some(("sonos", "uuid:mock-sonos"))`, `activation ==
    None`, error set. Pressing Enter on that row again clears
    `failed_output` and retries.
  - `pair_failure_stays_in_the_modal`: the error shows inside the modal;
    Esc → `activation == None`.
  - `done_for_a_vanished_row_clears_state` (Review Focus 5): the snapshot
    loses the activating row, then `Done { LABEL_OUTPUT, Err }` arrives →
    `activation == None`, and `failed_output` is set to the vanished id,
    which matches no remaining row.

- [ ] **Step 2: Write a failing test in `mock_core.rs`.**
  `bluetooth_pair_connect_activate`: drive `update` + `run_cmd` against
  the mock core for `bt-unpaired`, feeding each reply back. The chain
  ends with the active output being `bluetooth`/`bt-unpaired`.

- [ ] **Step 3:** Run the tests. Expected: FAIL.

- [ ] **Step 4: Implement.**
  `select` and `on_done` branch on `DestState` / `activation.step`.
  `pending` keeps a header label for each step: `LABEL_PAIR`,
  `LABEL_CONNECT`, `LABEL_OUTPUT`.

- [ ] **Step 5:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 6: Commit**

  ```bash
  git commit -am "feat(tui): one-at-a-time activation with Bluetooth pair and connect"
  ```

### Task 8: CD seek, go to track, eject, and the CD strip

**Files:**
- Modify:
  - `apps/tui/src/api.rs` (no change to `CdAction`)
  - `apps/tui/src/client.rs` (`control_cd`)
  - `apps/tui/src/update.rs`
  - `apps/tui/src/model.rs` (`Modal`)
  - `apps/tui/src/keys.rs`
  - `apps/tui/src/view/cd.rs`
  - `apps/tui/src/view/overlay.rs`
- Test: `update.rs`, `view/mod.rs`, `tests/mock_core.rs`

**Interfaces:**
- Produces:
  - `Cmd::ControlCd { action: CdAction, position_ms: Option<u64>, track: Option<u8> }`
    replaces `Cmd::ControlCd(CdAction)`
  - `pub async fn control_cd(&self, action: CdAction, position_ms: Option<u64>, track: Option<u8>) -> Result<(), ClientError>`
    sends `{"action", "position_ms"?, "track"?}`, omitting `None` fields
  - `Modal::TrackPrompt { input: String }` and `Modal::ConfirmEject`
  - `Action::SeekBack` (`[`), `Action::SeekForward` (`]`),
    `Action::GotoTrack` (`g`), `Action::Eject` (`e`)
  - `CdButton` hit rects and a `CdProgress` hit rect from `view/cd.rs`
  - `pub fn seek_target(position_ms: u64, duration_ms: u64, delta_ms: i64) -> u64` (clamped)

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `seek_keys_clamp`: at position 5_000 of 300_000, `[` → `ControlCd
    { Seek, Some(0), None }`. At 295_000, `]` → `Some(300_000)`.
  - `goto_prompt`:
    - `g` → `TrackPrompt`.
    - Typing `1`, `2`, Enter → `ControlCd { Goto, None, Some(12) }`, and the modal closes.
    - A non-digit is ignored. Enter on empty input does nothing.
    - A track above `track_count` sets the modal error
      "Track 12 is not on this disc", and nothing is sent.
  - `eject_confirms`:
    - `e` → `ConfirmEject`.
    - `y` → `ControlCd { Eject, None, None }`.
    - `n` or Esc → closes, nothing sent.
  - `cd_keys_need_a_disc`: with no disc, `[ ] g e` do nothing.
  - `prompt_closes_when_the_disc_goes` (Review Focus 3): with
    `TrackPrompt` or `ConfirmEject` open, a `CdStateChanged { present:
    false, .. }` closes the modal, and nothing is sent.

- [ ] **Step 2: Write failing tests in `view/mod.rs`.**

  - `cd_strip_with_disc`: contains `03/12`, the album and title,
    `02:14 / 09:22`, and `⏏`. The hit map has all four `CdButton`s and
    `CdProgress`.
  - `cd_strip_hidden_without_disc`.

- [ ] **Step 3: Write a failing test in `mock_core.rs`.**
  `cd_seek_goto_eject`: first insert a disc through the mock-only
  `POST /api/mock/cd` with body `{"present": true, "album": "Test",
  "tracks": [...]}` (see `CdSimulateRequest` in
  `packages/core/src/api/cd.rs`). Then `control_cd` with `Seek
  (position_ms 1000)`, `Goto (track 2)` and `Eject` each returns `Ok`, and
  `cd().present` is false afterwards.

- [ ] **Step 4:** Run the tests. Expected: FAIL.

- [ ] **Step 5: Implement.**
  Port existing `ControlCd(action)` callers to `{ action, None, None }`.
  Add the four keys to `KEYS` with help text, and update the key-count
  assertion.

- [ ] **Step 6:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 7: Commit**

  ```bash
  git commit -am "feat(tui): CD seek, go to track and eject with a clickable strip"
  ```

### Task 9: Status overlay, monitor-off panel, core-down handling

**Files:**
- Modify:
  - `apps/tui/src/model.rs` (`Modal::Status`, `core_down: bool`)
  - `apps/tui/src/update.rs`
  - `apps/tui/src/keys.rs` (`i`)
  - `apps/tui/src/view/overlay.rs`
  - `apps/tui/src/view/route.rs`
- Test: `update.rs`, `view/mod.rs`

**Interfaces:**
- Produces:
  - `Action::Status` (`i`), allowed while locked
  - `Model.core_down`: set on `Msg::Snapshot(Err)`, cleared on
    `Msg::Snapshot(Ok)`
  - `pub fn needs_core(cmd: &Cmd) -> bool` (everything except `Quit`,
    `QueryAutostart`, `SetAutostart`, `OpenBluetoothSettings`)

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `i_toggles_status_even_when_locked`.
  - `core_down_blocks_core_commands`: with `core_down`, Enter on a source
    → no commands, and `notice == Some("on-air-core is not reachable")`.
    `a` still works.
  - `snapshot_ok_clears_core_down`.

- [ ] **Step 2: Write failing tests in `view/mod.rs`.**

  - `status_overlay_lists_everything`: contains the version, `47990`,
    every LAN address as `addr:47990`, the AirPlay mode, `monitor on`,
    `sleep inhibited: yes`, `login start: on`, and the base URL. The base
    URL is passed through `Model.base: String`, set in `run_tui`.
  - `monitor_off_panel_replaces_route`: contains the three exact lines;
    Source and Destination titles are absent.
  - `core_down_dims_but_keeps_rows`: rows are still rendered (text
    present) with `Modifier::DIM`.

- [ ] **Step 3:** Run the tests. Expected: FAIL.

- [ ] **Step 4: Implement.**
  The `update` gate runs `needs_core` before returning commands when
  `core_down` is set. `Model.base` is added and set in `run_tui`.

- [ ] **Step 5:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 6: Commit**

  ```bash
  git commit -am "feat(tui): status overlay, monitor-off panel and core-down handling"
  ```

### Task 10: Sync: debounced refresh, background refresh, device placeholders

**Files:**
- Modify: `apps/tui/src/model.rs`, `apps/tui/src/update.rs`
- Test: `update.rs`

**Interfaces:**
- Produces:
  - `Model.refresh_in: Option<u8>` (frames until a debounced refresh)
  - `Model.since_refresh: u16` (frames since the last snapshot)
  - `Model.placeholders`, from Task 2, now populated

- [ ] **Step 1: Write failing tests.**

  - `events_debounce_into_one_refresh`:
    - Three `OutputStateChanged` events return no commands, and set
      `refresh_in == Some(6)`.
    - Five `Frame`s → nothing. The sixth → `[Refresh]`.
  - `device_joined_adds_placeholder_now`: `DeviceJoined { "sonos",
    "uuid:new", "New" }` → `dest_rows` contains `New` with
    `placeholder: true` before any refresh. `Snapshot(Ok)` clears
    `placeholders`.
  - `device_left_removes_row_now`: `DeviceLeft { "sonos",
    "uuid:mock-sonos" }` → the row is gone from `dest_rows` immediately.
  - `background_refresh_every_15s`: 300 `Frame`s with `Connection::Open`
    → `[Refresh]`, and the counter resets. With `Reconnecting`, nothing
    is sent.
  - `cd_and_monitor_events_unchanged`: the CD patch stays in place.
    `ServiceStateChanged` updates the status at once and schedules a
    debounced refresh.

- [ ] **Step 2:** Run the tests. Expected: FAIL.

- [ ] **Step 3: Implement.**
  `DeviceLeft` removes the row from `snapshot.outputs` and
  `snapshot.bluetooth`, and from `placeholders`. `since_refresh` resets
  on every `Snapshot(Ok)`. Rewrite the existing tests that expected an
  immediate `[Refresh]` from events.

- [ ] **Step 4:** Run `cargo test -p on-air-tui`. Expected: pass.

- [ ] **Step 5: Commit**

  ```bash
  git commit -am "feat(tui): debounced and background refresh, device placeholders"
  ```

### Task 11: Mouse input and the event loop

**Files:**
- Modify:
  - `apps/tui/src/update.rs` (`Msg::Mouse`)
  - `apps/tui/src/main.rs` (loop, mouse capture, frame tick, redraw throttle)
  - `apps/tui/src/view/overlay.rs` (help "Mouse" section)
  - `apps/tui/src/model.rs` (`mouse: bool`)
- Test: `update.rs`, `view/mod.rs`

**Interfaces:**
- Consumes:
  - `HitMap`, `Target`, `CdButton` (Task 3)
  - `fader_value` (Task 5)
  - `write` (Task 6)
  - `seek_target` (Task 8)
- Produces:
  - `pub enum MouseKind { Down, Drag, ScrollUp, ScrollDown }`
  - `Msg::Mouse { target: Target, rect: Rect, column: u16, row: u16, kind: MouseKind }`
  - `Model.mouse: bool` (whether capture is on)

- [ ] **Step 1: Write failing tests in `update.rs`.**

  - `click_source_row_selects_and_activates`:
    `Mouse { SourceRow(1), Down }` → `pane == Sources`,
    `source_index == 1`, and `[ActivateInput("Built-in mic")]`.
  - `click_dest_row_runs_enter_flow`: same as Enter in Task 7.
  - `drag_on_eq_fader_sets_value_through_write`:
    - `Fader(Eq(2))` with a rect of height 5, row = `rect.y` → band 2
      `= 12.0`, sent as `SetEq`.
    - Then a `Drag` at row `rect.y + 4` → queued `-12.0` (in flight).
  - `wheel_on_fader_fine_steps`: `ScrollUp` on `Fader(Volume)` → `+1`.
  - `wheel_on_list_moves_cursor`.
  - `click_rate_cycles`: `Rate(InputRate)` `Down` → the next catalog
    rate, via `write`.
  - `click_cd_controls`:
    - `CdButton(PlayPause)` behaves like Space.
    - `CdButton(Eject)` opens `ConfirmEject`.
    - `CdProgress` at column `rect.x + rect.width/2` →
      `Seek(duration/2 ± one column's worth)`.
  - `click_header_tags`: `MonitorTag` behaves like `m`; `StatusTag`
    opens the status overlay.
  - `click_outside_overlay_closes_it`: `OverlayOutside` with `Help` open
    → `Modal::None`.
  - `mouse_respects_lock_and_core_down`: while locked, only `MonitorTag`,
    `StatusTag` and `OverlayOutside` act.
  - `mouse_outside_every_target_is_ignored` (Review Focus 1): this one
    tests the loop helper. `pub fn mouse_msg(map: &HitMap, event:
    MouseEvent) -> Option<Msg>` in `main.rs` returns `None` for a cell
    that is in no rect, and for a cell beyond the map after a resize.
    Make it a `pub(crate)` function in a new `apps/tui/src/input.rs`
    so it can be tested.

- [ ] **Step 2: Write a failing test in `view/mod.rs`.**
  `help_has_mouse_section`: contains "Mouse" and "Hold Shift while
  dragging to select text.". With `model.mouse = false`, it contains
  "mouse off".

- [ ] **Step 3:** Run the tests. Expected: FAIL.

- [ ] **Step 4: Implement `update` and `input.rs`.**
  - `input::mouse_msg` maps crossterm `MouseEventKind::{Down(Left),
    Drag(Left), ScrollUp, ScrollDown}`. Everything else → `None`.
  - When an overlay is open, the overlay pushes `Overlay` for its own
    rect and `OverlayOutside` for the full screen *before* its own
    rect. Since the last pushed match wins, clicks inside the overlay
    hit `Overlay`.

- [ ] **Step 5: Implement the loop in `main.rs`.**
  - Run `crossterm::execute!(stdout, EnableMouseCapture)` after
    `ratatui::init()`. Set `model.mouse` from its result.
  - The panic hook and the normal exit run `DisableMouseCapture` before
    `ratatui::restore()`.
  - Add a 50 ms `frame` interval that sends `Msg::Frame`. The 1 s tick
    stays for theme reloads.
  - Keep `let mut hits = HitMap::default()`, and set it to the return
    value of each draw.
  - Redraw immediately after `Msg::Key` / `Msg::Mouse`. After any other
    message, set `dirty = true` and draw on the next `Msg::Frame`.

- [ ] **Step 6:** Run `cargo test -p on-air-tui` and clippy. Expected: pass.

- [ ] **Step 7: Commit**

  ```bash
  git commit -am "feat(tui): mouse support, frame tick and throttled redraw"
  ```

### Task 12: Docs and manual verification

**Files:**
- Modify: `README.md` (TUI key list), `docs/omarchy.md` (keys and mouse section)

- [ ] **Step 1: Document keys and mouse.**
  Add the key table from `keys.rs` (generated by hand from `KEYS`) and
  the mouse table from the spec to `docs/omarchy.md` under a "Using the
  TUI" heading. In `README.md`, add one line saying `i` shows status and
  the mouse works.

- [ ] **Step 2: Run the full checks.**

  ```bash
  cargo test -p on-air-tui -p on-air-core
  cargo clippy -p on-air-tui --all-targets -- -D warnings
  sh packaging/omarchy/test.sh
  ```

  Expected: all pass, `all ok`.

- [ ] **Step 3: Manual check at the target size.**
  1. Stop the installed service (`on-air service stop`). Run
     `ON_AIR_MOCK=1 cargo run -p on-air-core` and `cargo run -p on-air-tui`
     in a 100×30 terminal. Insert a mock disc with
     `curl -X POST localhost:47990/api/mock/cd -H 'content-type: application/json' -d '{"present":true}'`.
  2. Capture screenshots: idle, live, monitor off, status overlay, help,
     80×24 stacked, and 100×22 compact.
  3. Click a row, drag an EQ band, scroll volume, click the CD progress
     bar if a disc is mocked.

  Report anything that looks wrong before committing.

- [ ] **Step 4: Commit**

  ```bash
  git commit -am "docs: TUI keys, mouse and status overlay"
  ```
