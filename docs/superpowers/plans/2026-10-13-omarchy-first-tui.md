# Omarchy-first terminal UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Linux the Omarchy-first path of on-air: the headless core runs as a systemd user service that owns the sleep inhibitor, a Rust terminal UI replaces the WebKitGTK window on Linux, and an Arch package ships the launcher entry, bar module, menu and keybinding snippets.

**Architecture:** The core (`packages/core`) absorbs the sleep inhibitor and the service on/off transition from the Tauri shell and exposes `PUT /api/service`. A new crate `apps/tui` (binary `on-air`) is a loopback HTTP and WebSocket client built as an Elm-style model/update/view with a pure `update`, plus `bar`, `status` and `service` subcommands. `packaging/omarchy` and `packaging/aur/on-air` ship the unit, desktop entry, icon, snippets and PKGBUILD.

**Tech Stack:** Rust 2021 (cargo 1.97), axum 0.7 and tokio in the core; ratatui 0.30, crossterm 0.29 (`event-stream`), reqwest 0.12 (`default-features = false`, `json`), tokio-tungstenite 0.30, serde, serde_json, futures-util in the TUI; POSIX sh, systemd user units, PKGBUILD.

**Spec:** `docs/superpowers/specs/2026-10-13-omarchy-first-tui-design.md`

## Global Constraints

- The TUI crate never depends on `on-air-core` at runtime; it is a `dev-dependency` only (for the in-process mock core in integration tests).
- Every binary, package, desktop entry, icon and the Wayland app id are named `on-air`; the app id is `org.omarchy.on-air`.
- TUI backgrounds are `Color::Reset`; foregrounds are the sixteen named ANSI colors and `BOLD`, `DIM`, `REVERSED`; the only non-ANSI color is the accent from Omarchy's `colors.toml`.
- The TUI reads only `colors.toml` under `~/.local/state/omarchy/current/theme/` then `~/.config/omarchy/current/theme/`, and writes nothing under `~/.config/omarchy`.
- Default base URL `http://127.0.0.1:47990`; read timeout 5 s; slow calls (`activateOutput`, pairing, connect, service change) 60 s; WebSocket backoff 500 ms to 10 s with 20 % jitter and a 20 s stall timer.
- Error copy strings are the ones in `apps/desktop/src/lib/errorCopy.ts`, verbatim.
- Linux inhibitor stays `systemd-inhibit --what=sleep:idle --who=on-air --why=Keep the LAN audio service discoverable --mode=block sleep infinity`; macOS `caffeinate -i -s -w <pid>`; Windows `SetThreadExecutionState`.
- Every non-2xx response is the `{error, code}` envelope; the new route returns `400 invalid_request` on a bad body.
- `cargo fmt --all -- --check` and `cargo clippy --locked -p on-air-core -p on-air-tui --all-targets -- -D warnings` are clean before every commit. Commit messages are imperative, prefixed `feat(core):`, `feat(tui):`, `feat(packaging):`, `docs:` or `ci:`.
- Nothing in the Tauri shell changes behaviour on macOS or Windows.

## Review Focus

1. A `WsEvent` with a `type` the TUI has never seen must be ignored, not crash the client or drop the connection (Task 7 adds the test).
2. A snapshot refresh that fails halfway (for example `/api/cd` returns 503 while `/api/status` succeeded) must keep the parts that loaded and show one error banner (Task 8).
3. A device name with multibyte or very long text must not break the bar JSON or the pane layout: the JSON is escaped by `serde_json`, the pane truncates on display width (Tasks 9 and 10).
4. The terminal must be restored (raw mode off, main screen) when the TUI panics or the core goes away mid-session (Task 11's panic hook; checked by the smoke test there: kill the mock core while the TUI runs, then `q`, and the shell prompt must come back intact).
5. `PUT /api/service` called twice concurrently, or while the output is still switching, must not double-release the inhibitor or leave the flag and the audio state disagreeing (Task 2).

---

## File structure

**Core (`packages/core`)**
- Create `src/keep_awake.rs` — moved from the Tauri shell; adds `is_holding()`.
- Modify `src/state.rs` — `keep_awake` field, `ServiceTransition`, `begin_service_transition`, `finish_service_transition`, `sleep_inhibited`.
- Modify `src/lib.rs` — `StatusResponse.sleep_inhibited`, route `PUT /api/service`, `pub mod keep_awake`.
- Create `src/api/service.rs` — the route handler and its request/response structs.
- Modify `src/sender/bluetooth_host.rs` — Omarchy candidates in `open_settings`.
- Create `tests/api_service.rs` — route tests.

**Tauri shell (`apps/desktop/src-tauri`)**
- Delete `src/keep_awake.rs`; modify `src/lib.rs` to use the core's transition and `KeepAwake`.

**TUI (`apps/tui`)** — one responsibility per file:
- `Cargo.toml`, `README.md`
- `src/main.rs` — argument parsing, dispatch to TUI or CLI, terminal lifecycle, event loop.
- `src/api.rs` — mirrored JSON types.
- `src/client.rs` — `Client` (HTTP) and `spawn_events` (WebSocket) producing `Msg`s.
- `src/model.rs` — `Model`, `Snapshot`, `Connection`, `Pane`, `Modal`, `Pending`.
- `src/update.rs` — `Msg`, `Cmd`, `update()`.
- `src/keys.rs` — `Action`, `KEYS` table, `action_for(KeyEvent, &Model) -> Option<Action>`.
- `src/view.rs` — `view()`, layout and widgets.
- `src/error_copy.rs` — code to copy.
- `src/theme.rs` — `Theme { accent }`, `load()`, `refresh()`.
- `src/autostart.rs` — systemctl wrapper (Linux).
- `src/cli.rs` — `bar`, `status`, `service` subcommands.
- `tests/mock_core.rs` — in-process mock core golden path.

**Packaging**
- `packaging/omarchy/{on-air-core.service,on-air.desktop,on-air.svg,install.sh,test.sh}` and `snippets/`.
- `packaging/aur/on-air/{PKGBUILD,.SRCINFO}`.

**Workflows and docs**
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `packaging/update-checksums.sh`.
- `README.md`, `INSTALL.md`, `CONTEXT.md`, `docs/omarchy.md`, `apps/desktop/README.md`, `packaging/README.md`.

---

### Task 1: Move the sleep inhibitor into the core

**Files:**
- Create: `packages/core/src/keep_awake.rs` (from `apps/desktop/src-tauri/src/keep_awake.rs`)
- Modify: `packages/core/src/lib.rs` (add `pub mod keep_awake;`)
- Modify: `packages/core/src/state.rs:85-160` (field), `:160-240` (constructors)
- Delete: `apps/desktop/src-tauri/src/keep_awake.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs:16-23` (imports), `:425` (`app.manage(KeepAwake::system(..))` removed)

**Interfaces:**
- Consumes: the existing `KeepAwake`, `InhibitMethod`, `InhibitorSpawner`, `Inhibitor`, `SystemSpawner`, `platform_inhibit_method()`.
- Produces:
  - `on_air_core::keep_awake::{KeepAwake, InhibitMethod, InhibitorSpawner, Inhibitor, platform_inhibit_method}` with the same signatures as today, plus `pub fn is_holding(&self) -> bool` (true while a spawned inhibitor is alive; false when disabled, when the platform has no method, or when the spawn returned `None`).
  - `pub mod testing` is `pub` and unconditional (no `cfg(test)`: the crate's own integration tests and the Tauri shell's tests compile the library without `cfg(test)`, and the module is a few dozen lines). `recording(enabled) -> (KeepAwake, RecordingSpawner)`, `fake_method()`, `Event::{Started, Released}` keep their shapes. `RecordingSpawner` gains `pub fn refusing() -> Self` whose `spawn` returns `None`.
  - `CoreState` gains `pub keep_awake: Arc<KeepAwake>`; `from_saved_settings` creates `KeepAwake::system(saved_settings.service_enabled)`; `pub fn with_keep_awake(mut self, keep_awake: KeepAwake) -> Self` replaces it (used by tests and by Task 2's route tests).
  - `pub fn sleep_inhibited(&self) -> bool` on `CoreState` = `self.keep_awake.is_holding()`.

- [ ] **Step 1: Move the file**

```bash
git mv apps/desktop/src-tauri/src/keep_awake.rs packages/core/src/keep_awake.rs
```

Add `pub mod keep_awake;` to `packages/core/src/lib.rs` next to the other `pub mod` lines. In `keep_awake.rs` change `#[cfg(test)] pub(crate) mod testing` to `pub mod testing` (drop the `cfg`), and change the `tests` module's `use super::testing::…` accordingly. `is_enabled()` loses its `#[cfg(test)]` and becomes a plain `pub fn`.

- [ ] **Step 2: Write the failing tests for `is_holding` and the refusing spawner**

Append to the `tests` module in `packages/core/src/keep_awake.rs`:

```rust
#[test]
fn is_holding_follows_the_live_inhibitor() {
    let (keep_awake, _spawner) = recording(true);
    assert!(keep_awake.is_holding());
    keep_awake.set_enabled(false);
    assert!(!keep_awake.is_holding());
}

#[test]
fn a_refused_inhibitor_is_enabled_but_not_holding() {
    let spawner = testing::RecordingSpawner::refusing();
    let keep_awake = KeepAwake::with_spawner(Some(fake_method()), Box::new(spawner.clone()), true);
    assert!(keep_awake.is_enabled());
    assert!(!keep_awake.is_holding());
    assert_eq!(spawner.events(), vec![Event::Started(fake_method())]);
}

#[test]
fn no_platform_method_is_never_holding() {
    let keep_awake = KeepAwake::with_spawner(None, Box::new(testing::RecordingSpawner::default()), true);
    assert!(!keep_awake.is_holding());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p on-air-core keep_awake`
Expected: compile error, `is_holding` and `refusing` not found.

- [ ] **Step 4: Implement `is_holding` and `RecordingSpawner::refusing`**

`is_holding(&self) -> bool` is `self.active.lock().unwrap().is_some()`. `refusing()` builds a `RecordingSpawner` with a `refuse: bool` field whose `spawn` records `Started` and returns `None`.

- [ ] **Step 5: Run the keep_awake tests to verify they pass**

Run: `cargo test -p on-air-core keep_awake`
Expected: all keep_awake tests PASS, including the four moved ones.

- [ ] **Step 6: Write the failing CoreState test**

In the `tests` module of `packages/core/src/state.rs`:

```rust
#[test]
fn a_new_state_holds_the_inhibitor_only_while_the_service_is_on() {
    let (keep_awake, spawner) = crate::keep_awake::testing::recording(true);
    let state = CoreState::new().with_keep_awake(keep_awake);
    assert!(state.sleep_inhibited());
    assert_eq!(spawner.events().len(), 1);
}
```

- [ ] **Step 7: Run it to verify it fails**

Run: `cargo test -p on-air-core a_new_state_holds`
Expected: compile error, `with_keep_awake` not found.

- [ ] **Step 8: Add the field, constructor wiring, `with_keep_awake` and `sleep_inhibited`**

In `from_saved_settings` build `keep_awake: Arc::new(KeepAwake::system(saved_settings.service_enabled))`. `with_keep_awake` replaces the `Arc`. `CoreState::new_mock()` must not spawn a real `systemd-inhibit` on CI: after `Self::new()` it calls `.with_keep_awake(KeepAwake::with_spawner(None, Box::new(keep_awake::NoopSpawner), true))`, where `pub struct NoopSpawner;` (always `None`) is added next to `SystemSpawner` in `keep_awake.rs`.

- [ ] **Step 9: Run the core tests**

Run: `cargo test -p on-air-core`
Expected: PASS.

- [ ] **Step 10: Point the Tauri shell at the core's module**

In `apps/desktop/src-tauri/src/lib.rs`: remove `mod keep_awake;`, replace `use keep_awake::KeepAwake;` with `use on_air_core::keep_awake::KeepAwake;`, and in the `tests` module `use on_air_core::keep_awake::testing::{fake_method, recording, Event};`. Keep `app.manage(KeepAwake::system(service_on))` for now; Task 3 removes it.

- [ ] **Step 11: Verify the workspace builds and passes**

Run: `cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked -p on-air-core && cargo test --locked -p on-air-desktop --lib`
Expected: PASS. (The desktop `--lib` tests need the Linux Tauri deps listed in `INSTALL.md`; if they are missing locally, run them in CI and note it in the commit.)

- [ ] **Step 12: Commit**

```bash
git add -A packages/core apps/desktop/src-tauri Cargo.lock
git commit -m "feat(core): own the sleep inhibitor in the core"
```

---

### Task 2: Service transition in the core and `PUT /api/service`

**Files:**
- Modify: `packages/core/src/state.rs:427-431` (`set_service_enabled` stays; add the transition API after it)
- Create: `packages/core/src/api/service.rs`
- Modify: `packages/core/src/api/mod.rs` (add `pub mod service;`)
- Modify: `packages/core/src/lib.rs:33-52` (`StatusResponse`), `:63-105` (route)
- Test: `packages/core/tests/api_service.rs`, `packages/core/tests/status_router.rs`

**Interfaces:**
- Consumes: Task 1's `KeepAwake`, `CoreState::keep_awake`, `sleep_inhibited()`.
- Produces (in `on_air_core::state`):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceTransition { Unchanged, TurnOn, TurnOff }
pub fn service_transition(current: bool, requested: bool) -> ServiceTransition;
impl CoreState {
    /// Persist the flag, broadcast ServiceStateChanged, hold or release the inhibitor. Sync.
    pub fn begin_service_transition(&self, enabled: bool) -> ServiceTransition;
    /// TurnOff: self.shutdown().await. TurnOn: self.spawn_saved_session_restore(). Unchanged: nothing.
    pub async fn finish_service_transition(&self, transition: ServiceTransition);
}
```

  `StatusResponse` gains `pub sleep_inhibited: bool`; `status_with_service(service_enabled: bool)` becomes `status_for(state: &CoreState)`, and `status()` is removed if nothing else calls it (grep first). Route `PUT /api/service`: `SetServiceRequest { enabled: bool }` → `200 Json(ServiceResponse { enabled: bool })`, extractor order `(Paired, State(state), JsonBody(body))`.

- [ ] **Step 1: Write the failing route tests**

`packages/core/tests/api_service.rs`, reusing the `mock_app`/`send` helpers from `tests/api_bluetooth.rs` (copy them; the tests directory has no shared module). A `put_json(uri, body)` helper mirrors `post_json` with method `PUT`.

```rust
#[tokio::test]
async fn turning_the_service_off_and_on_round_trips_and_broadcasts() {
    let (app, state) = mock_app().await;
    let mut events = state.ws_tx.subscribe();
    let (status, body) = send(app.clone(), put_json("/api/service", r#"{"enabled":false}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    assert!(!state.service_enabled.load(Ordering::Acquire));
    assert!(matches!(events.try_recv(), Ok(WsEvent::ServiceStateChanged { enabled: false })));
    let (_, status_body) = send(app.clone(), Request::builder().uri("/api/status").body(Body::empty()).unwrap()).await;
    assert_eq!(status_body["service_enabled"], false);
    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":true}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert!(matches!(events.try_recv(), Ok(WsEvent::ServiceStateChanged { enabled: true })));
}

#[tokio::test]
async fn a_repeated_request_is_idempotent_and_emits_nothing() {
    let (app, state) = mock_app().await;
    let mut events = state.ws_tx.subscribe();
    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":true}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn a_bad_body_is_a_400_envelope() {
    let (app, _) = mock_app().await;
    let (status, body) = send(app, put_json("/api/service", r#"{"enabled":"yes"}"#)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request");
}

#[tokio::test]
async fn turning_off_stops_the_live_output() {
    let (app, state) = mock_app().await;
    send(app.clone(), post_json("/api/outputs/active", r#"{"transport":"sonos","device_id":"uuid:mock-sonos"}"#)).await;
    assert!(state.active_output().is_some());
    send(app.clone(), put_json("/api/service", r#"{"enabled":false}"#)).await;
    assert!(state.active_output().is_none());
}

#[tokio::test]
async fn concurrent_toggles_leave_flag_and_inhibitor_in_step() {
    let (keep_awake, spawner) = on_air_core::keep_awake::testing::recording(true);
    let state = CoreState::new_mock().await.with_keep_awake(keep_awake);
    let app = on_air_core::build_router(state.clone());
    let off = send(app.clone(), put_json("/api/service", r#"{"enabled":false}"#));
    let off_again = send(app.clone(), put_json("/api/service", r#"{"enabled":false}"#));
    tokio::join!(off, off_again);
    assert!(!state.service_enabled.load(Ordering::Acquire));
    assert_eq!(spawner.events(), vec![Event::Started(fake_method()), Event::Released], "one release, not two");
    assert!(!state.sleep_inhibited());
}
```

The last test imports `use on_air_core::keep_awake::testing::{fake_method, Event};`. Also extend `tests/status_router.rs::status_route_returns_ok_json` with `assert!(json["sleep_inhibited"].is_boolean());`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-core --test api_service --test status_router`
Expected: FAIL (404 on `/api/service`; `sleep_inhibited` missing).

- [ ] **Step 3: Implement the transition API in `state.rs`**

Move `ServiceTransition` and `service_transition` from the Tauri shell verbatim. `begin_service_transition`: compute the transition from `self.service_enabled`; if `Unchanged` return it; otherwise `self.set_service_enabled(enabled)`, `let _ = self.ws_tx.send(WsEvent::ServiceStateChanged { enabled })`, `self.keep_awake.set_enabled(enabled)`. The flag read and store must be one atomic step so two concurrent `TurnOff`s cannot both see `true`: use `compare_exchange` on `service_enabled` to claim the transition, then persist via `update_saved_settings`.

- [ ] **Step 4: Implement the route and status field**

`src/api/service.rs`:

```rust
#[derive(Deserialize)] pub struct SetServiceRequest { pub enabled: bool }
#[derive(Serialize)]   pub struct ServiceResponse { pub enabled: bool }
pub async fn set_service(Paired: Paired, State(state): State<CoreState>, JsonBody(body): JsonBody<SetServiceRequest>) -> Json<ServiceResponse>
```

It calls `begin_service_transition` then `finish_service_transition(..).await` and returns the flag. Register `.route("/api/service", put(api::service::set_service))` in `build_router`. Replace `status_with_service` with `status_for(&state)` filling `service_enabled` and `sleep_inhibited`.

- [ ] **Step 5: Run the core tests**

Run: `cargo test -p on-air-core`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/core
git commit -m "feat(core): PUT /api/service and the service transition"
```

---

### Task 3: Tauri shell uses the core's transition

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs:76-160` (`ServiceTransition`, `service_transition`, `apply_service_transition`, `apply_service_enabled`), `:425` (manage), `:520-545` (exit), `:566-620` (tests)

**Interfaces:**
- Consumes: Task 2's `ServiceTransition`, `begin_service_transition`, `finish_service_transition`, `CoreState::keep_awake`.
- Produces: none new; `set_service_enabled` command and tray item behave as before.

- [ ] **Step 1: Rewrite the shell's transition tests against the core API**

Replace `turning_the_service_off_and_on_updates_core_clients_and_inhibitor` with a test of the same name that builds `CoreState::new().with_keep_awake(keep_awake)` and calls `state.begin_service_transition(false)` / `(true)`, asserting the same events and spawner log. Replace the `keep_awake` argument of `finish_exit` with `&CoreState` and call `state.keep_awake.set_enabled(false)` inside; update `exit_stops_the_core_then_releases_the_inhibitor_then_exits` accordingly. Delete `a_request_for_the_current_state_changes_nothing` (now covered in the core).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-desktop --lib`
Expected: compile errors on the removed names.

- [ ] **Step 3: Delete the shell's copies and call the core**

Remove `ServiceTransition`, `service_transition`, `apply_service_transition`, `struct ServiceMenu` stays. `apply_service_enabled` becomes: `let transition = state.begin_service_transition(enabled);` then the existing menu-label, `service-changed` emit, and for `TurnOff` spawn `state.finish_service_transition(transition)` with the menu disabled meanwhile; for `TurnOn` call `tauri::async_runtime::spawn(async move { state.finish_service_transition(transition).await })`. Remove `app.manage(KeepAwake::system(service_on))` and every `try_state::<KeepAwake>()`; the exit path uses `core.keep_awake`.

- [ ] **Step 4: Run the shell tests and clippy**

Run: `cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test -p on-air-desktop --lib`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri
git commit -m "feat(desktop): drive the service through the core transition"
```

---

### Task 4: Omarchy candidates for the Bluetooth settings handoff

**Files:**
- Modify: `packages/core/src/sender/bluetooth_host.rs:244-258`

**Interfaces:**
- Produces: `pub const SETTINGS_CANDIDATES: &[(&str, &[&str])]` (Linux module) in the order the spec fixes; `open_settings()` iterates it.

- [ ] **Step 1: Write the failing test**

In the Linux `platform` module's tests (create a `#[cfg(all(test, target_os = "linux"))] mod tests` if none):

```rust
#[test]
fn omarchy_panels_come_before_desktop_settings() {
    let programs: Vec<&str> = SETTINGS_CANDIDATES.iter().map(|(bin, _)| *bin).collect();
    assert_eq!(programs, vec!["omarchy-shell", "omarchy-launch-tui", "gnome-control-center", "blueman-manager", "kcmshell6", "kcmshell5"]);
    assert_eq!(SETTINGS_CANDIDATES[0].1, &["shell", "toggle", "omarchy.bluetooth"]);
    assert_eq!(SETTINGS_CANDIDATES[1].1, &["bluetui"]);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p on-air-core omarchy_panels`
Expected: compile error, `SETTINGS_CANDIDATES` not found.

- [ ] **Step 3: Lift the candidate list to a `pub const` with the two Omarchy entries first**

Update the error message to "could not open Bluetooth settings (tried Omarchy, GNOME, Blueman, KDE)".

- [ ] **Step 4: Run the test**

Run: `cargo test -p on-air-core omarchy_panels`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/core/src/sender/bluetooth_host.rs
git commit -m "feat(core): open the Omarchy Bluetooth panel before desktop settings"
```

---

### Task 5: TUI crate scaffold, API types and HTTP client

**Files:**
- Modify: `Cargo.toml` (workspace members gain `"apps/tui"`)
- Create: `apps/tui/Cargo.toml`, `apps/tui/src/main.rs`, `apps/tui/src/api.rs`, `apps/tui/src/client.rs`, `apps/tui/src/error_copy.rs`
- Test: `apps/tui/tests/mock_core.rs`

**Interfaces:**
- Produces (`apps/tui/src/api.rs`, all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, field names and optionality exactly as `packages/api-types/src/index.ts`): `StatusResponse` (with `sleep_inhibited: Option<bool>` and `lan_addresses: Option<Vec<String>>`), `PinResponse`, `InputsResponse`, `ActiveInputResponse`, `OutputInfo`, `OutputsResponse`, `ActiveOutput`, `OutputPhase`, `ActiveOutputView { #[serde(flatten)] pub output: ActiveOutput, pub state: Option<OutputPhase> }`, `ActiveOutputResponse`, `OutputVolumeResponse`, `EqResponse`, `SampleRateSide`, `SampleRateResponse`, `AirPlayModeResponse`, `BluetoothDeviceInfo`, `BluetoothListResponse`, `CdTrackInfo`, `CdStatus`, `CdAction`, `ApiErrorBody`, `ServiceResponse`, and `WsEvent` as `#[serde(tag = "type")]` with an `#[serde(other)] Unknown` variant.
- Produces (`apps/tui/src/client.rs`):

```rust
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(60);
#[derive(Debug)] pub enum ClientError { Unreachable(String), Timeout, Http { status: u16, code: Option<String>, message: String } }
#[derive(Clone)] pub struct Client { base: String, http: reqwest::Client }
impl Client {
    pub fn new(base: &str) -> Self;           // trims a trailing '/'
    pub fn base(&self) -> &str;
    pub async fn status(&self) -> Result<StatusResponse, ClientError>;
    pub async fn pin(&self) -> Result<String, ClientError>;
    pub async fn inputs(&self) -> Result<Vec<String>, ClientError>;
    pub async fn active_input(&self) -> Result<ActiveInputResponse, ClientError>;
    pub async fn activate_input(&self, name: &str) -> Result<(), ClientError>;
    pub async fn outputs(&self) -> Result<Vec<OutputInfo>, ClientError>;
    pub async fn active_output(&self) -> Result<Option<ActiveOutputView>, ClientError>;
    pub async fn activate_output(&self, transport: &str, device_id: &str) -> Result<(), ClientError>; // SLOW_TIMEOUT
    pub async fn deactivate_output(&self) -> Result<(), ClientError>;
    pub async fn volume(&self) -> Result<u8, ClientError>;
    pub async fn set_volume(&self, volume: u8) -> Result<(), ClientError>;
    pub async fn eq(&self) -> Result<[f32; 5], ClientError>;
    pub async fn set_eq(&self, gains_db: [f32; 5]) -> Result<(), ClientError>;
    pub async fn sample_rate(&self) -> Result<SampleRateResponse, ClientError>;
    pub async fn set_sample_rate(&self, input_hz: Option<u32>, output_hz: Option<u32>) -> Result<(), ClientError>;
    pub async fn airplay_mode(&self) -> Result<String, ClientError>;
    pub async fn pair_airplay(&self, device_id: &str, pin: &str) -> Result<(), ClientError>; // SLOW_TIMEOUT
    pub async fn bluetooth_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, ClientError>;
    pub async fn pair_bluetooth(&self, id: &str) -> Result<(), ClientError>;    // SLOW_TIMEOUT
    pub async fn connect_bluetooth(&self, id: &str) -> Result<(), ClientError>; // SLOW_TIMEOUT
    pub async fn open_bluetooth_settings(&self) -> Result<(), ClientError>;
    pub async fn cd(&self) -> Result<CdStatus, ClientError>;
    pub async fn control_cd(&self, action: CdAction) -> Result<(), ClientError>;
    pub async fn set_service(&self, enabled: bool) -> Result<bool, ClientError>; // SLOW_TIMEOUT
}
```

  Routes and bodies are those of `packages/control-client/src/http.ts`. A non-2xx response parses `ApiErrorBody` into `Http { code, message }`; a connect failure is `Unreachable(base)`; a timeout is `Timeout`.
- Produces (`apps/tui/src/error_copy.rs`): `pub fn copy(error: &ClientError) -> String` returning the `COPY_BY_CODE` sentence for the code, else the `COPY_BY_STATUS` sentence, else `format!("The audio service answered {status}.")`; `Timeout` → `"The audio service did not answer in time."`; `Unreachable(base)` → `format!("Core unreachable on {base}; is on-air-core running? (systemctl --user status on-air-core)")`.

- [ ] **Step 1: Create the crate**

`apps/tui/Cargo.toml`:

```toml
[package]
name = "on-air-tui"
version = "0.1.0"
edition = "2021"
description = "on-air terminal UI and CLI for the on-air-core service"

[[bin]]
name = "on-air"
path = "src/main.rs"

[dependencies]
ratatui = "0.30"
crossterm = { version = "0.29", features = ["event-stream"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "process"] }
tokio-tungstenite = "0.30"
reqwest = { version = "0.12", default-features = false, features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
futures-util = "0.3"
unicode-width = "0.2"

[dev-dependencies]
on-air-core = { path = "../../packages/core" }
```

Add `"apps/tui"` to the workspace `members` in the root `Cargo.toml`. `src/main.rs` is a placeholder `fn main() { println!("on-air"); }` until Task 11. Run `cargo build -p on-air-tui` and commit `Cargo.lock`.

- [ ] **Step 2: Write the failing integration test**

`apps/tui/tests/mock_core.rs` starts a mock core in-process:

```rust
use on_air_core::state::CoreState;
use on_air_tui::client::Client;

async fn mock_core() -> Client {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = CoreState::new_mock().await;
    tokio::spawn(async move { on_air_core::serve_with_state(listener, state).await.unwrap() });
    Client::new(&format!("http://{addr}"))
}

#[tokio::test]
async fn golden_path_sonos() {
    let c = mock_core().await;
    assert_eq!(c.status().await.unwrap().status, "ok");
    assert_eq!(c.pin().await.unwrap(), "123456");
    let inputs = c.inputs().await.unwrap();
    assert_eq!(inputs, vec!["Mock Monitor"]);
    c.activate_input("Mock Monitor").await.unwrap();
    let outputs = c.outputs().await.unwrap();
    let sonos = outputs.iter().find(|o| o.transport == "sonos").unwrap();
    c.activate_output("sonos", &sonos.id).await.unwrap();
    assert_eq!(c.active_output().await.unwrap().unwrap().output.device_id, "uuid:mock-sonos");
    c.set_volume(20).await.unwrap();
    assert_eq!(c.volume().await.unwrap(), 20);
    c.set_eq([3.0, 0.0, 0.0, 0.0, -3.0]).await.unwrap();
    assert_eq!(c.eq().await.unwrap(), [3.0, 0.0, 0.0, 0.0, -3.0]);
    c.deactivate_output().await.unwrap();
    assert!(c.active_output().await.unwrap().is_none());
}

#[tokio::test]
async fn service_off_is_reported_and_blocks_audio() {
    let c = mock_core().await;
    assert!(!c.set_service(false).await.unwrap());
    assert!(!c.status().await.unwrap().service_enabled);
    let err = c.activate_input("Mock Monitor").await.unwrap_err();
    assert!(matches!(err, on_air_tui::client::ClientError::Http { status: 503, .. }));
    assert_eq!(on_air_tui::error_copy::copy(&err), "The audio service is paused. Turn it on to continue.");
    assert!(c.set_service(true).await.unwrap());
}

#[tokio::test]
async fn unreachable_core_is_a_named_error() {
    let c = Client::new("http://127.0.0.1:1");
    let err = c.status().await.unwrap_err();
    assert!(matches!(err, on_air_tui::client::ClientError::Unreachable(_)));
    assert!(on_air_tui::error_copy::copy(&err).starts_with("Core unreachable on http://127.0.0.1:1;"));
}
```

This needs the crate to expose a library: add `[lib] path = "src/lib.rs"` to `Cargo.toml` with `pub mod api; pub mod client; pub mod error_copy;` (later tasks add their modules), and `main.rs` uses `on_air_tui::…`.

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p on-air-tui --test mock_core`
Expected: compile error, modules missing.

- [ ] **Step 4: Implement `api.rs`, `client.rs`, `error_copy.rs`**

One private `async fn request<T: DeserializeOwned>(&self, method, path, body: Option<impl Serialize>, timeout) -> Result<T, ClientError>` does the work; `204` and empty bodies deserialize to `()` via a `T = ()` special case. `error_copy.rs` holds the two maps as `match` arms.

- [ ] **Step 5: Run the test**

Run: `cargo test -p on-air-tui --test mock_core`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock apps/tui
git commit -m "feat(tui): crate scaffold, mirrored API types and loopback client"
```

---

### Task 6: WebSocket subscription

**Files:**
- Modify: `apps/tui/src/client.rs`
- Test: `apps/tui/tests/mock_core.rs`

**Interfaces:**
- Produces:

```rust
pub const STALL: Duration = Duration::from_secs(20);
pub const MIN_BACKOFF: Duration = Duration::from_millis(500);
pub const MAX_BACKOFF: Duration = Duration::from_secs(10);
pub fn backoff_delay(attempt: u32, jitter: f64) -> Duration;   // 500ms * 2^attempt capped at 10 s, ±20 % by `jitter` in [-1, 1]
#[derive(Debug, Clone, PartialEq)] pub enum Connection { Connecting, Open, Reconnecting, Closed }
pub enum Incoming { Connection(Connection), Event(WsEvent) }
pub fn spawn_events(base: &str, tx: tokio::sync::mpsc::UnboundedSender<Incoming>) -> tokio::task::JoinHandle<()>;
```

  `spawn_events` turns `http(s)://` into `ws(s)://` + `/api/ws`, sends `Connection(Connecting)`, `Open` on connect, `Reconnecting` on close or stall, and forwards every parsed `WsEvent` (including `Unknown`, which callers drop). It never ends on its own; the caller aborts the handle.

- [ ] **Step 1: Write the failing tests**

Unit test in `client.rs`:

```rust
#[test]
fn backoff_doubles_and_caps() {
    assert_eq!(backoff_delay(0, 0.0), Duration::from_millis(500));
    assert_eq!(backoff_delay(1, 0.0), Duration::from_millis(1000));
    assert_eq!(backoff_delay(10, 0.0), Duration::from_secs(10));
    assert_eq!(backoff_delay(0, 1.0), Duration::from_millis(600));
    assert_eq!(backoff_delay(0, -1.0), Duration::from_millis(400));
}
```

Integration test in `tests/mock_core.rs`:

```rust
#[tokio::test]
async fn events_arrive_after_a_service_change() {
    let c = mock_core().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = on_air_tui::client::spawn_events(c.base(), tx);
    assert!(matches!(rx.recv().await, Some(Incoming::Connection(Connection::Connecting))));
    assert!(matches!(rx.recv().await, Some(Incoming::Connection(Connection::Open))));
    c.set_service(false).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop { if let Some(Incoming::Event(WsEvent::ServiceStateChanged { enabled })) = rx.recv().await { break enabled; } }
    }).await.unwrap();
    assert!(!event);
    handle.abort();
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui backoff events_arrive`
Expected: compile errors.

- [ ] **Step 3: Implement `backoff_delay` and `spawn_events`**

The task loop: connect with `tokio_tungstenite::connect_async`; on success reset `attempt` to 0 and read frames under `tokio::time::timeout(STALL, ..)` per frame; text frames parse with `serde_json::from_str::<WsEvent>` and unknown `type`s become `Unknown`; a parse error on a frame is logged to stderr and skipped. On error or stall send `Reconnecting` and sleep `backoff_delay(attempt, rand)` where `rand` comes from a simple xorshift seeded by the process id (no `rand` crate).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p on-air-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): WebSocket events with reconnect and stall detection"
```

---

### Task 7: Model, messages and the pure update

**Files:**
- Create: `apps/tui/src/model.rs`, `apps/tui/src/update.rs`, `apps/tui/src/keys.rs`
- Modify: `apps/tui/src/lib.rs`

**Interfaces:**
- Produces (`model.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Pane { Sources, Outputs, Mixer }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum MixerRow { Volume, Eq(usize) /* 0..5 */, InputRate, OutputRate }
#[derive(Debug, Clone, PartialEq)] pub enum PairTarget { Airplay { device_id: String, name: String }, Bluetooth { id: String, name: String } }
#[derive(Debug, Clone, PartialEq)] pub enum Modal { None, Help, Pin { target: PairTarget, input: String, error: Option<String> } }
#[derive(Debug, Clone, PartialEq)] pub struct Pending { pub label: String }
#[derive(Debug, Clone, Default, PartialEq)] pub struct Snapshot { pub status: Option<StatusResponse>, pub pin: Option<String>, pub inputs: Vec<String>, pub active_input: Option<String>, pub outputs: Vec<OutputInfo>, pub active_output: Option<ActiveOutputView>, pub volume: u8, pub eq: [f32; 5], pub rates: Option<SampleRateResponse>, pub cd: Option<CdStatus>, pub airplay_mode: Option<String> }
#[derive(Debug, Clone, PartialEq)] pub struct Model { pub connection: Connection, pub snapshot: Snapshot, pub pane: Pane, pub source_index: usize, pub output_index: usize, pub mixer_row: MixerRow, pub modal: Modal, pub pending: Option<Pending>, pub error: Option<String>, pub notice: Option<String>, pub autostart: Option<bool>, pub accent: ratatui::style::Color, pub size: (u16, u16), pub quit: bool }
impl Model { pub fn new() -> Self; pub fn service_on(&self) -> bool; pub fn locked(&self) -> bool /* !service_on */; }
```

  Output rows are grouped by transport in the order sonos, airplay, bluetooth; `pub fn output_rows(&self) -> Vec<OutputRow>` with `enum OutputRow { Header(&'static str), Device(usize /* index into snapshot.outputs */) }`; `output_index` indexes `output_rows()` and `j`/`k` skip headers.
- Produces (`update.rs`):

```rust
pub enum Msg { Key(crossterm::event::KeyEvent), Resize(u16, u16), Tick, Ws(Incoming), Snapshot(Result<Snapshot, String>), Done { label: String, result: Result<(), String> }, Autostart(Result<bool, String>), Accent(Color) }
#[derive(Debug, Clone, PartialEq)] pub enum Cmd { Refresh, ActivateInput(String), ActivateOutput { transport: String, device_id: String }, DeactivateOutput, SetVolume(u8), SetEq([f32; 5]), SetSampleRate { input_hz: Option<u32>, output_hz: Option<u32> }, ControlCd(CdAction), PairAirplay { device_id: String, pin: String }, PairBluetooth(String), ConnectBluetooth(String), OpenBluetoothSettings, SetService(bool), QueryAutostart, SetAutostart(bool), Quit }
pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd>;
```

- Produces (`keys.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Action { NextPane, PrevPane, Down, Up, Select, AdjustUp, AdjustDown, VolumeUp, VolumeDown, ResetBand, Stop, PlayPause, NextTrack, PrevTrack, Service, Autostart, Bluetooth, Refresh, Help, Back, Quit }
pub struct Binding { pub keys: &'static str, pub action: Action, pub help: &'static str }
pub const KEYS: &[Binding];          // one row per line of the spec's key table, in that order
pub fn action_for(key: &KeyEvent) -> Option<Action>;
```

- [ ] **Step 1: Write the failing update tests**

In `update.rs` `tests`, with a helper `fn model_with_mock_snapshot() -> Model` (one input "Mock Monitor", outputs Sonos "uuid:mock-sonos"/"Mock Sonos", AirPlay "ap-living"/"Living Room AirPlay" (`needs_pair: false`), Bluetooth "bt-speaker" (`paired: true`) and "bt-unpaired" (`paired: false, needs_pair: true`), volume 50, eq zeros, rates input 48000 of [44100, 48000], output 44100 of [44100, 48000, 96000], status `service_enabled: true`) and `fn key(c: char) -> Msg`:

```rust
#[test] fn tab_cycles_panes_and_wraps() { /* Sources → Outputs → Mixer → Sources; Shift-Tab reverse */ }
#[test] fn j_and_k_skip_output_headers() { /* first j lands on Device(0), never on Header */ }
#[test] fn enter_on_a_source_activates_it() { assert_eq!(update(&mut m, key('\n')), vec![Cmd::ActivateInput("Mock Monitor".into())]); assert_eq!(m.pending.as_ref().unwrap().label, "Switching source"); }
#[test] fn enter_on_a_paired_output_activates_it() { /* Cmd::ActivateOutput { transport: "sonos", device_id: "uuid:mock-sonos" } */ }
#[test] fn enter_on_an_unpaired_bluetooth_output_opens_the_pin_modal() { /* modal is Modal::Pin { target: PairTarget::Bluetooth { id: "bt-unpaired", .. }, input: "", error: None }, no Cmd */ }
#[test] fn pin_modal_accepts_digits_submits_on_enter_and_closes_on_esc() { /* 'a' ignored; "1234" + Enter → Cmd::PairBluetooth / PairAirplay{pin:"1234"}; Esc → Modal::None */ }
#[test] fn a_failed_pairing_keeps_the_modal_with_the_error() { /* Msg::Done{label:"Pairing", result: Err("That PIN did not match…")} → modal still Pin with error Some */ }
#[test] fn h_and_l_adjust_the_focused_mixer_row() { /* Volume 50 → l → SetVolume(55); Eq(2) l → SetEq with +1.0 at index 2; InputRate l → SetSampleRate{input_hz: Some(48000)→ next in catalog wraps to 44100}; bounds: volume never above 100 or below 0, EQ clamped to ±12 */ }
#[test] fn plus_and_minus_change_volume_from_any_pane() { /* in Sources pane '+' → SetVolume(55) */ }
#[test] fn zero_resets_the_focused_band() { /* Eq(1) at 3.0 → '0' → SetEq with 0.0 at index 1 */ }
#[test] fn x_stops_the_live_output() { /* active_output Some → 'x' → DeactivateOutput; None → no Cmd */ }
#[test] fn cd_keys_only_act_with_a_disc() { /* cd None: Space → []; cd present → Space → ControlCd(Play or Pause by `playing`); '>' Next; '<' Prev */ }
#[test] fn s_toggles_the_service_and_locks_the_panes() { /* 's' → SetService(false); after Ws ServiceStateChanged{false}: locked(), 'j' → [] and Enter → [] */ }
#[test] fn a_toggles_autostart() { /* autostart Some(false) → 'a' → SetAutostart(true) */ }
#[test] fn ws_events_refresh_or_patch_the_snapshot() { /* OutputStateChanged → [Cmd::Refresh]; ServiceStateChanged{enabled} patches status.service_enabled and → [Refresh]; CdStateChanged patches cd; DeviceJoined/DeviceLeft → [Refresh]; LevelMeter and Heartbeat → [] */ }
#[test] fn unknown_ws_events_are_ignored() { assert_eq!(update(&mut m, Msg::Ws(Incoming::Event(WsEvent::Unknown))), vec![]); }
#[test] fn connection_changes_keep_the_snapshot() { /* Ws(Connection(Reconnecting)) → connection Reconnecting, snapshot unchanged; Connection(Open) → [Cmd::Refresh] */ }
#[test] fn a_failed_snapshot_sets_the_error_and_keeps_the_old_one() { /* Msg::Snapshot(Err("…")) → error Some, snapshot unchanged */ }
#[test] fn done_clears_pending_and_errors_show_once() { /* Done Ok → pending None, error None; Done Err(copy) → error Some(copy), Esc clears it */ }
#[test] fn q_quits_and_help_toggles() { /* '?' → Modal::Help; Esc → None; 'q' → quit true and [Cmd::Quit]; '?' then 'q' closes help first */ }
#[test] fn resize_and_accent_are_recorded() { /* Resize(120, 40) → size; Accent(Color::Rgb(1,2,3)) → accent */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui update`
Expected: compile errors.

- [ ] **Step 3: Implement `model.rs`, `keys.rs`, `update.rs`**

`update` is a `match` on `Msg`; `Msg::Key` first routes to the modal when one is open (Help: any key closes; Pin: digits, Backspace, Enter, Esc), then maps through `action_for` and a `match` per `(Action, pane)`. Locked model: only `Service`, `Autostart`, `Help`, `Quit`, `Back` act. The EQ step is 1.0 dB clamped to `[-12.0, 12.0]`; volume step 5 clamped to `[0, 100]`; the rate rows step through `rates.input.supported_hz` / `rates.output.supported_hz` with wrap. Pending labels: "Switching source", "Switching output", "Stopping", "Pairing", "Connecting", "Changing service", "Refreshing".

- [ ] **Step 4: Run the tests**

Run: `cargo test -p on-air-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): model, key map and pure update"
```

---

### Task 8: Snapshot loading and command execution

**Files:**
- Modify: `apps/tui/src/client.rs` (add `load_snapshot`, `run_cmd`)
- Test: `apps/tui/tests/mock_core.rs`

**Interfaces:**
- Produces:

```rust
impl Client {
    /// Every read in parallel; a part that fails is left at its default and its copy is collected.
    pub async fn load_snapshot(&self) -> (Snapshot, Vec<String>);
    /// Executes one Cmd; returns the Msg to feed back (Done, Snapshot or Autostart).
    pub async fn run_cmd(&self, cmd: Cmd) -> Option<Msg>;
}
```

  `load_snapshot` returns `Err` to the model (as `Msg::Snapshot(Err(copy))`) only when `status` itself failed; otherwise `Ok(snapshot)` and the partial-failure copies are joined into `Msg::Done { label: "Refreshing", result: Err(joined) }` by the caller. `run_cmd` maps `Cmd::Refresh` to `load_snapshot`, `Cmd::SetAutostart`/`QueryAutostart` to Task 10's `autostart` module, `Cmd::Quit` to `None`, and everything else to the matching `Client` method followed by `Done { label, result }` where `label` is the pending label from Task 7 and `result` carries `error_copy::copy`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_snapshot_loads_every_part_from_the_mock_core() {
    let c = mock_core().await;
    let (snap, failures) = c.load_snapshot().await;
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(snap.pin.as_deref(), Some("123456"));
    assert_eq!(snap.inputs, vec!["Mock Monitor"]);
    assert_eq!(snap.outputs.iter().filter(|o| o.transport == "bluetooth").count(), 2);
    assert_eq!(snap.volume, 50);
    assert!(snap.rates.is_some());
    assert_eq!(snap.cd.as_ref().map(|cd| cd.present), Some(false));
}

#[tokio::test]
async fn a_paused_core_still_yields_the_parts_that_load() {
    let c = mock_core().await;
    c.set_service(false).await.unwrap();
    let (snap, failures) = c.load_snapshot().await;
    assert_eq!(snap.status.as_ref().map(|s| s.service_enabled), Some(false));
    assert_eq!(snap.pin.as_deref(), Some("123456"), "pairing does not need the service");
    assert!(!failures.is_empty() || !snap.inputs.is_empty(), "either inputs load or the failure is reported");
}

#[tokio::test]
async fn run_cmd_round_trips_an_activation() {
    let c = mock_core().await;
    let msg = c.run_cmd(Cmd::ActivateOutput { transport: "sonos".into(), device_id: "uuid:mock-sonos".into() }).await;
    assert!(matches!(msg, Some(Msg::Done { ref label, result: Ok(()) }) if label == "Switching output"));
    let msg = c.run_cmd(Cmd::ActivateOutput { transport: "sonos".into(), device_id: "nope".into() }).await;
    assert!(matches!(msg, Some(Msg::Done { result: Err(ref copy), .. }) if copy == "That device is no longer available. Refresh devices and try again."));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui --test mock_core`
Expected: compile errors.

- [ ] **Step 3: Implement `load_snapshot` with `tokio::join!` and `run_cmd`**

- [ ] **Step 4: Run the tests**

Run: `cargo test -p on-air-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): snapshot loading and command execution"
```

---

### Task 9: View

**Files:**
- Create: `apps/tui/src/view.rs`
- Modify: `apps/tui/src/lib.rs`

**Interfaces:**
- Consumes: `Model`, `KEYS`.
- Produces: `pub fn view(model: &Model, frame: &mut ratatui::Frame)`; `pub const MIN_SIZE: (u16, u16) = (40, 10)`; `pub const STACK_BELOW_COLUMNS: u16 = 80`; `pub fn truncate(text: &str, width: usize) -> String` (display width, appends `…` when cut).

- [ ] **Step 1: Write the failing render tests**

In `view.rs` `tests`, with `fn render(model: &Model, w: u16, h: u16) -> ratatui::buffer::Buffer` using `TestBackend` and `Terminal::draw`, and `fn text(buffer) -> String` joining rows:

```rust
#[test] fn the_header_shows_live_output_pin_and_service() { /* contains "● live", "Mock Sonos", "PIN 123456", "service on" */ }
#[test] fn the_header_says_service_off() { /* "service off — press s" */ }
#[test] fn the_header_warns_when_sleep_is_not_inhibited() { /* status.sleep_inhibited Some(false) and service on → "sleep not inhibited" */ }
#[test] fn the_header_shows_unreachable() { /* connection Closed → contains "unreachable" */ }
#[test] fn the_focused_row_is_marked() { /* "> Mock Monitor" in Sources */ }
#[test] fn outputs_are_grouped_under_transport_headers() { /* "Sonos" before "Mock Sonos", "AirPlay" before "Living Room AirPlay", "Bluetooth" before "Mock Bluetooth Speaker"; unpaired shows "(pair)" */ }
#[test] fn the_mixer_shows_volume_eq_and_rates() { /* "Volume", "EQ 60", "EQ 12k", "+2.0 dB" when eq[2] = 2.0, "In 48000", "Out 44100" */ }
#[test] fn the_cd_row_appears_only_with_a_disc() { /* "Audio CD" and "03/12" only when cd.present */ }
#[test] fn the_hint_line_lists_the_key_table() { /* last row contains "? help" and "q quit" */ }
#[test] fn below_min_size_only_the_resize_notice_renders() { /* 30x5 → text contains "resize", not "Sources" */ }
#[test] fn narrow_terminals_stack_the_panes() { /* 60x40 → "Sources" row index < "Outputs" row index < "Mixer" row index, all at column 1 */ }
#[test] fn long_names_are_truncated_to_the_pane() { /* a 60-char device name in a 100x30 render ends with "…" and no row exceeds 100 cells */ }
#[test] fn the_pin_modal_and_help_overlay_render_centered() { /* "Pair" + target name + typed digits; Help contains every KEYS.help string */ }
#[test] fn backgrounds_are_reset_everywhere() { /* every cell's bg == Color::Reset */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui view`
Expected: compile errors.

- [ ] **Step 3: Implement `view.rs`**

Layout with `Layout::vertical([Length(1) header, Length(cd? 1 : 0), Length(error? 1 : 0), Length(notice? 1 : 0), Min(0) panes, Length(1) hints])`; panes `Layout::horizontal` with `[Percentage(25), Percentage(35), Percentage(40)]` or vertical thirds when `width < STACK_BELOW_COLUMNS`. Selection style is `Style::new().fg(model.accent).add_modifier(REVERSED)` with the pane's `List` highlight symbol `"> "`; live marker green, pending yellow, errors red, locked panes `DIM`. Bars are `Gauge`-free: a string of `█`/`░` of width 10 for volume and a 9-cell band with `█` at the gain position for EQ (so `TestBackend` text assertions hold).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p on-air-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): render the header, panes, modals and hints"
```

---

### Task 10: Theme accent, autostart and CLI subcommands

**Files:**
- Create: `apps/tui/src/theme.rs`, `apps/tui/src/autostart.rs`, `apps/tui/src/cli.rs`
- Modify: `apps/tui/src/lib.rs`, `apps/tui/src/client.rs` (`run_cmd` wires autostart)
- Test: `apps/tui/tests/mock_core.rs`

**Interfaces:**
- Produces (`theme.rs`):

```rust
pub struct Theme { dir: Option<PathBuf>, mtime: Option<SystemTime>, pub accent: Color }
pub const FALLBACK_ACCENT: Color = Color::Cyan;
pub fn candidate_dirs(home: &Path) -> [PathBuf; 2]; // .local/state/omarchy/current/theme, .config/omarchy/current/theme
pub fn parse_accent(colors_toml: &str) -> Option<Color>; // the `accent = "#rrggbb"` line, any section; hand parsed, no toml crate
impl Theme { pub fn load(home: &Path) -> Self; pub fn refresh(&mut self) -> bool /* true when accent changed */; }
```

- Produces (`autostart.rs`, `#[cfg(target_os = "linux")]`; other targets return `Err("autostart is managed by the desktop app on this platform")`):

```rust
pub const UNIT: &str = "on-air-core";
pub fn systemctl_args(enabled: bool) -> Vec<&'static str>; // ["--user","enable","--now",UNIT] or ["--user","disable",UNIT]
pub async fn is_enabled() -> Result<bool, String>;   // `systemctl --user is-enabled on-air-core`, exit 0 → true
pub async fn set_enabled(enabled: bool) -> Result<bool, String>;
```

- Produces (`cli.rs`):

```rust
pub struct BarLine { pub text: String, pub tooltip: String, pub class: &'static str }
pub fn bar_line(status: Option<&StatusResponse>, active: Option<&ActiveOutputView>, volume: Option<u8>) -> BarLine;
pub async fn bar(client: &Client) -> String;          // serde_json of BarLine, always exit 0
pub async fn status(client: &Client, json: bool) -> Result<String, ClientError>;
pub async fn service(client: &Client, arg: &str) -> Result<String, ClientError>; // "on" | "off" | "toggle" → prints "service on"/"service off"
```

  `bar_line` states: `status None` → text `󰝛`, class `down`, tooltip "on-air-core is not running"; `service_enabled false` → `󰝛 off`, `off`; no active or state != live → `󰝚`, `idle`; live → `󰝚 <device_name>`, `live`; tooltip for live is `"<transport> · <device_name> · vol <volume> · <first lan address>"`.

- [ ] **Step 1: Write the failing tests**

`theme.rs` tests: `parse_accent("[colors]\naccent = \"#ffd60a\"\n") == Some(Color::Rgb(255, 214, 10))`, a missing key → `None`, a short hex → `None`; `load` on a temp home with no dir → `FALLBACK_ACCENT`; writing `colors.toml` into the first candidate dir then `refresh()` → true and the parsed color; same content again → `refresh()` false.
`autostart.rs` test: `systemctl_args(true) == ["--user","enable","--now","on-air-core"]`, `systemctl_args(false) == ["--user","disable","on-air-core"]`.
`cli.rs` tests: one per bar state asserting `text` and `class`, plus `bar_line` with a name containing `"` and `é` serialising to valid JSON (`serde_json::from_str::<serde_json::Value>` succeeds and `["text"]` ends with the name).
`tests/mock_core.rs`:

```rust
#[tokio::test]
async fn bar_and_service_subcommands_against_the_mock_core() {
    let c = mock_core().await;
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "idle");
    assert_eq!(on_air_tui::cli::service(&c, "toggle").await.unwrap(), "service off");
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "off");
    assert_eq!(on_air_tui::cli::service(&c, "on").await.unwrap(), "service on");
    c.activate_output("sonos", "uuid:mock-sonos").await.unwrap();
    let line: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&c).await).unwrap();
    assert_eq!(line["class"], "live");
    assert_eq!(line["text"], "󰝚 Mock Sonos");
    let down: serde_json::Value = serde_json::from_str(&on_air_tui::cli::bar(&Client::new("http://127.0.0.1:1")).await).unwrap();
    assert_eq!(down["class"], "down");
    assert!(on_air_tui::cli::status(&c, true).await.unwrap().contains("\"service_enabled\""));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui`
Expected: compile errors.

- [ ] **Step 3: Implement the three modules and wire `Cmd::QueryAutostart`/`SetAutostart` in `run_cmd` to `Msg::Autostart`**

`status(.., json=false)` prints one line per snapshot part in the header's wording; `json=true` prints `serde_json::to_string_pretty` of the `Snapshot`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p on-air-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): theme accent, systemd autostart and bar/status/service subcommands"
```

---

### Task 11: Entry point and event loop

**Files:**
- Modify: `apps/tui/src/main.rs`
- Create: `apps/tui/README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: `pub fn parse_args(args: &[String]) -> Result<Invocation, String>` in `main.rs` (tested), with `pub enum Invocation { Tui { base: String }, Bar { base: String }, Status { base: String, json: bool }, Service { base: String, arg: String }, Version, Help }`; default base `http://127.0.0.1:47990`; `--base URL` accepted anywhere; unknown flags are `Err` with the usage text.

- [ ] **Step 1: Write the failing `parse_args` tests**

```rust
#[test] fn no_args_is_the_tui_on_the_default_base() { /* Invocation::Tui { base: "http://127.0.0.1:47990" } */ }
#[test] fn subcommands_and_base_parse() { /* ["bar","--base","http://10.0.0.2:47990"] → Bar{base}; ["status","--json"] → Status{json:true}; ["service","toggle"] → Service{arg:"toggle"}; ["--version"] → Version */ }
#[test] fn unknown_input_is_an_error_with_usage() { /* ["frobnicate"] → Err containing "usage:" ; ["service","maybe"] → Err */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p on-air-tui --bin on-air`
Expected: compile errors.

- [ ] **Step 3: Implement `main.rs`**

`#[tokio::main]`: parse; CLI invocations call `cli::*`, print, exit 1 on `Err` with `error_copy::copy`; `Version` prints `env!("CARGO_PKG_VERSION")`. The TUI path: install a panic hook that calls `ratatui::restore()` then the default hook; `let mut terminal = ratatui::init();` (raw mode, alternate screen); `Model::new()` with `accent` from `Theme::load(home)` and `size` from `terminal.size()`; spawn `spawn_events`; an `mpsc::unbounded_channel::<Msg>()`; the loop `tokio::select!` over `crossterm::event::EventStream` (Key → `Msg::Key`, Resize → `Msg::Resize`), the WS channel, the Msg channel and a 1 s `tokio::time::interval` tick (which also runs `theme.refresh()` and sends `Msg::Accent` on change). After every message, `update` returns `Cmd`s; each is spawned as `client.run_cmd(cmd)` whose `Some(msg)` is sent back on the Msg channel; `Cmd::Quit` breaks the loop. Draw with `terminal.draw(|f| view(&model, f))` after each message. On exit `ratatui::restore()` and abort the WS handle. The initial commands are `[Cmd::Refresh, Cmd::QueryAutostart]`.

- [ ] **Step 4: Run the tests, then a manual smoke test against the mock core**

Run: `cargo test -p on-air-tui`
Expected: PASS.
Run (two terminals): `ON_AIR_MOCK=1 PORT=47990 cargo run -p on-air-core --bin on-air-core` and `cargo run -p on-air-tui`. Expected: the screen from the spec; `Tab`, `j`, `Enter` on "Mock Sonos" shows `● live`; `q` restores the terminal. Then `cargo run -p on-air-tui -- bar` prints the live JSON line.

- [ ] **Step 5: Write `apps/tui/README.md`**

Dev loop (the two commands above), test commands, the key table copied from the spec, and the subcommands.

- [ ] **Step 6: Commit**

```bash
git add apps/tui
git commit -m "feat(tui): entry point, event loop and README"
```

---

### Task 12: Omarchy packaging files

**Files:**
- Create: `packaging/omarchy/on-air-core.service`, `on-air.desktop`, `on-air.svg`, `snippets/{omarchy-menu.jsonc,bindings.lua,bindings.conf,hyprland-rules.lua,hyprland-rules.conf,shell-bar.json,waybar.jsonc}`, `install.sh`, `test.sh`
- Create: `packaging/aur/on-air/PKGBUILD`, `packaging/aur/on-air/.SRCINFO`

**Interfaces:**
- Consumes: the `on-air` and `on-air-core` binaries.
- Produces: files whose contents are fixed by the spec's "Omarchy packaging and integration" section (unit, desktop entry, menu rows, bindings, bar entries verbatim).

- [ ] **Step 1: Write `packaging/omarchy/test.sh` first**

POSIX sh, `set -eu`, run from the repo root. Checks, each printing `ok <file>` or failing: `systemd-analyze --user verify packaging/omarchy/on-air-core.service` when `systemd-analyze` exists; `desktop-file-validate packaging/omarchy/on-air.desktop` when it exists, else `grep -q '^Exec=xdg-terminal-exec --app-id=org.omarchy.on-air -e on-air$'`; `python3 -c 'import xml.dom.minidom,sys; xml.dom.minidom.parse(sys.argv[1])' packaging/omarchy/on-air.svg`; `sed 's#//.*##' snippets/omarchy-menu.jsonc | sed '1s/^/{/;$s/$/}/' | jq . >/dev/null` when `jq` exists (the snippet is a fragment: rows ending in commas; strip the last comma with `sed '$ s/,$//'`); `jq . snippets/shell-bar.json`; `luac -p snippets/*.lua` when `luac` exists; `sh -n packaging/omarchy/install.sh`; `grep -q "org.omarchy.on-air" snippets/hyprland-rules.lua snippets/hyprland-rules.conf`.

- [ ] **Step 2: Run it to verify it fails**

Run: `sh packaging/omarchy/test.sh`
Expected: FAIL on the first missing file.

- [ ] **Step 3: Write the files**

Unit, desktop entry, menu rows, bindings and bar entries: copy from the spec verbatim. `hyprland-rules.lua`: `o.window("org.omarchy.on-air", { tag = "+floating-window" })`; `hyprland-rules.conf`: `windowrule = tag +floating-window, match:class ^(org\.omarchy\.on-air)$`. `waybar.jsonc`: `"custom/on-air": { "exec": "on-air bar", "return-type": "json", "interval": 5, "on-click": "omarchy-launch-or-focus-tui on-air" }`. `on-air.svg`: 64×64 viewBox, `<rect width="64" height="64" rx="14" fill="#ffd60a"/>` and the live-dial mark (a lowercase "a" glyph path with a dot above, traced from `apps/desktop/src-tauri/icons/icon.png` by eye; white fill). `install.sh`: `set -eu`; `cd packaging/aur/on-air && makepkg -si`; `systemctl --user daemon-reload && systemctl --user enable --now on-air-core`; then `for f in /usr/share/doc/on-air/omarchy/*; do echo "== $f"; cat "$f"; done` with a one-line header per file naming the destination path (menu → `~/.config/omarchy/extensions/omarchy-menu.jsonc`, bindings.lua → `~/.config/hypr/bindings.lua`, bindings.conf → `~/.config/hypr/bindings.conf`, hyprland-rules.* → `~/.config/hypr/hyprland.lua` / `hyprland.conf`, shell-bar.json → `~/.config/omarchy/shell.json` under `bar.layout.right`, waybar.jsonc → `~/.config/waybar/config.jsonc`).

- [ ] **Step 4: Write the PKGBUILD and `.SRCINFO`**

Per the spec's Arch package section: `pkgname=on-air`, `pkgver=0.1.0`, `pkgrel=1`, `pkgdesc='LAN audio to AirPlay, Bluetooth or Sonos: core service and terminal UI'`, `arch=('x86_64' 'aarch64')`, `url`, `license=('custom:unknown')`, `depends`, `optdepends`, `makedepends=('cargo' 'git')`, `provides`, `conflicts`, `options=('!debug' '!lto')`, `source=("${pkgname}-${pkgver}::git+https://github.com/tsonubin/on-air.git#tag=v${pkgver}")`, `sha256sums=('SKIP')`, `prepare`, `build`, `check`, `package` installing the table's files with `install -Dm755`/`-Dm644`. `.SRCINFO` by hand in the same shape as a `makepkg --printsrcinfo` output (there is no `makepkg` here; note that in the commit).

- [ ] **Step 5: Run the packaging test**

Run: `sh packaging/omarchy/test.sh`
Expected: every line `ok`.

- [ ] **Step 6: Commit**

```bash
git add packaging
git commit -m "feat(packaging): Omarchy unit, launcher entry, snippets and Arch recipe"
```

---

### Task 13: CI and release

**Files:**
- Modify: `.github/workflows/ci.yml:52-68` (test-core job), `:120-150` (check-desktop already covers the workspace)
- Modify: `.github/workflows/release.yml:117-118` (build), `:171-210` (stage)
- Modify: `packaging/update-checksums.sh:46-54`

**Interfaces:**
- Produces: release assets `on-air-tui-linux-x86_64` and `on-air-tui-linux-aarch64`.

- [ ] **Step 1: Extend the `test-core` job**

Rename it `fmt / clippy / core and tui tests`; `Lint core` → `cargo clippy --locked -p on-air-core -p on-air-tui --all-targets -- -D warnings`; `Test core` → `cargo test --locked -p on-air-core -p on-air-tui`; add a step `Validate Omarchy packaging` running `sudo apt-get install -y desktop-file-utils jq lua5.4 && sh packaging/omarchy/test.sh`.

- [ ] **Step 2: Extend the release matrix for Linux**

After `Build headless core binary`, add `- name: Build terminal UI` with `if: matrix.os_name == 'linux'` running `cargo build --locked -p on-air-tui --release --bin on-air --target ${{ matrix.rust_target }}`. In `Stage extra artifacts`, in the non-Windows branch, when `os_name = linux` copy `target/${rust_target}/release/on-air` to `dist-extra/on-air-tui-${os_name}-${arch_name}` and fail if missing.

- [ ] **Step 3: Teach the checksum script about the new assets**

Add `tui_x86=$(try_hash "on-air-tui-linux-x86_64" || true)` and `tui_arm=$(try_hash "on-air-tui-linux-aarch64" || true)` next to the other `try_hash` lines so a missing asset is reported.

- [ ] **Step 4: Validate the workflow files**

Run: `python3 -c 'import yaml,sys; [yaml.safe_load(open(p)) for p in sys.argv[1:]]' .github/workflows/ci.yml .github/workflows/release.yml && sh -n packaging/update-checksums.sh`
Expected: no output (valid YAML, valid sh). If PyYAML is unavailable, `pip install pyyaml` in the scratchpad venv first.

- [ ] **Step 5: Commit**

```bash
git add .github packaging/update-checksums.sh
git commit -m "ci: lint, test and package the terminal UI; ship tui binaries in releases"
```

---

### Task 14: Documentation

**Files:**
- Modify: `README.md`, `INSTALL.md`, `CONTEXT.md`, `apps/desktop/README.md`, `packaging/README.md`
- Create: `docs/omarchy.md`

**Interfaces:**
- Consumes: the spec's "Docs" section, which fixes each file's content.

- [ ] **Step 1: Write `docs/omarchy.md`**

Sections, in order: Install (the `install.sh` line, then the manual `makepkg -si` + `systemctl --user enable --now on-air-core`), Launch (launcher entry, `Super+Space`, the app id, the floating rule snippet and where it goes for Omarchy 4 and 3), Keybinding (both snippets), Menu (the jsonc rows), Bar (shell.json entry and the Waybar module), Idle and sleep (the shell's idle lock still fires; suspend is blocked by logind while the service is on; check with `on-air status` → `sleep inhibited: yes`; fallback `omarchy-toggle-idle stay-awake` and a polkit rule example granting `org.freedesktop.login1.inhibit-block-sleep` to the user), Old Macs (nouveau + Intel renderer env from the research for the 2013/2014 15-inch models: `~/.config/uwsm/env.d/20-intel-renderer` with `AQ_NO_KMS_REQUIREMENT=1` and `AQ_DRM_DEVICES`, `omarchy toggle animations off`), Uninstall (`systemctl --user disable --now on-air-core`, `sudo pacman -Rns on-air`, remove the snippets).

- [ ] **Step 2: Update the existing docs**

`README.md` first paragraph and the stack sentence: Linux path is the terminal UI over the user service; Tauri for macOS and Windows; add `cargo run -p on-air-tui` to the command block. `INSTALL.md`: new first section "Omarchy / Arch (recommended on Linux)" before "Desktop GUI", the one-line demotion note in the Debian, Arch and Fedora sections, the two `on-air-tui-linux-*` assets in the expected-assets list and the GitHub Releases table, and the packager map row `| Omarchy / Arch | packaging/omarchy/, packaging/aur/on-air | packaging/omarchy/install.sh |`. `CONTEXT.md`: the **Control surface** and **Service** definitions from the spec. `apps/desktop/README.md`: one line that on Linux the recommended UI is `apps/tui`. `packaging/README.md`: the new recipe in the "After a GitHub Release" list as source-based (no hashes) and an "Omarchy" subsection pointing at `docs/omarchy.md`.

- [ ] **Step 3: Check the docs render and the Biome formatter is happy**

Run: `pnpm format:check` (Biome covers JSON/TS only; it must stay clean) and `grep -n "on-air-tui-linux" INSTALL.md packaging/update-checksums.sh`
Expected: Biome clean; both files mention the assets.

- [ ] **Step 4: Commit**

```bash
git add README.md INSTALL.md CONTEXT.md docs/omarchy.md apps/desktop/README.md packaging/README.md
git commit -m "docs: Omarchy-first install and integration guide"
```

---

### Task 15: Whole-branch verification

- [ ] **Step 1: Run every check a contributor runs**

```bash
cargo fmt --all -- --check
cargo clippy --locked -p on-air-core -p on-air-tui --all-targets -- -D warnings
cargo test --locked -p on-air-core -p on-air-tui
sh packaging/omarchy/test.sh
pnpm check:ci
pnpm typecheck:web
```

Expected: all clean. The Tauri crate is checked by CI's `check-desktop` matrix; run `cargo clippy --locked --workspace --all-targets -- -D warnings` locally too if the WebKitGTK dev packages are installed.

- [ ] **Step 2: Smoke test the TUI once more against the mock core (Task 11 Step 4) and push**

```bash
git push -u origin shay/serene-noether-jx5udl
```
