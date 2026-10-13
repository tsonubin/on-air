# on-air — Omarchy-first design: terminal UI over a service

**Date:** 2026-10-13
**Status:** Approved in conversation; spec under review

## Why

on-air will mostly run on Omarchy Linux (Arch + Hyprland), on a 2014 MacBook
Pro. The current Linux desktop is a Tauri 2 shell, which on Linux means a
WebKitGTK web view: a GPU-composited browser engine. On that hardware under
Hyprland it lags. The product does not need a browser engine: the Rust core
already runs headless with a complete HTTP and WebSocket control plane on
port 47990, and the React page is a thin layer over it. Only three things
live in the Tauri shell and not in the core: the service on/off toggle, the
sleep inhibitor, and login autostart.

Omarchy 4 (current stable, "Quattro") treats two kinds of apps as
first-class: native windows and terminal UIs. TUIs get their own launcher
(`omarchy-launch-tui`), a floating centered terminal window rule for the
`TUI.float` app id, automatic theming from the terminal palette, launcher
entries, menu rows and keybindings. Flatpak and AppImage are not supported;
apps are Arch packages with a `.desktop` file. Omarchy 4 replaced Waybar,
Walker, mako, hypridle and hyprlock with one Quickshell process
("omarchy-shell"), and Hyprland config moved to Lua; Omarchy 3 is still in
the field. Suspend is blocked with `systemd-inhibit --what=sleep` through
logind on both.

The decision, approved in conversation: an Omarchy-first path made of the
headless core running as a systemd user service plus a Rust terminal UI,
packaged as an Arch package with full Omarchy integration. The Tauri desktop
stays for macOS and Windows; its Linux bundles keep building but are no
longer the recommended Linux path.

## Goals

1. A Linux control surface with no browser engine and no GPU toolkit:
   smooth on a 2014 MacBook Pro.
2. Everything the desktop page does today, available from the terminal UI:
   sources, outputs, volume, EQ, sample rates, CD transport, pairing PIN,
   AirPlay and Bluetooth pairing, service on/off, autostart.
3. Streaming survives closing the UI. The service, not the window, owns
   playback, discovery and the sleep inhibitor.
4. First-class Omarchy integration: Arch package, launcher entry as a
   floating terminal, menu rows, a keybinding, a bar status module, and
   theme-following with zero theme code of our own.
5. Works unchanged on Omarchy 3 and 4 and on plain Arch with Hyprland.

## Non-goals

- Removing the Linux Tauri bundles or the `on-air-bin` AUR recipe. They stay
  and are demoted in the docs.
- A native GTK or Qt window. The TUI is the Linux GUI; a native window is a
  separate decision if the TUI proves insufficient.
- Changes to the mobile app, the macOS and Windows desktop behaviour, or the
  audio pipeline.
- A shared Rust API-types crate or `ts-rs` generation. The TUI mirrors the
  JSON shapes by hand, exactly as `@on-air/control-client` does, and an
  integration test against the mock core keeps them honest.
- Editing the user's Omarchy shell, menu or Hyprland config from a script.
  The package ships snippets and the docs say where they go.
- A tray icon. The bar module and the launcher entry replace it on Omarchy.

## Architecture

```
┌──────────────────────────┐   loopback HTTP + WS    ┌──────────────────────┐
│ on-air (TUI, ratatui)    │ ───────────────────────▶│ on-air-core          │
│ apps/tui                 │                         │ systemd --user unit  │
│ runs in a floating       │   systemctl --user      │ holds the sleep      │
│ terminal, no daemon      │ ──────(autostart)──────▶│ inhibitor            │
└──────────────────────────┘                         └──────────┬───────────┘
┌──────────────────────────┐   loopback HTTP (poll)             │ LAN 47990
│ on-air-bar (shell script)│ ───────────────────────────────────┤
│ Omarchy bar / Waybar     │                                    ▼
└──────────────────────────┘                        phone remote, Sonos, …
```

Three units, each with one job:

- **Core service** (`packages/core`, binary `on-air-core`): unchanged control
  plane, plus ownership of the sleep inhibitor and a route to toggle the
  service. Runs under `systemd --user` on Linux.
- **Terminal UI** (`apps/tui`, binary `on-air`): a loopback client. Holds no
  audio state; everything it shows comes from the core. Exits without
  touching playback.
- **Bar module** (`packaging/omarchy/on-air-bar`): a POSIX shell script that
  polls the core and prints one line of Waybar-style JSON.

## Core changes (`packages/core`)

### Sleep inhibitor moves into the core

`apps/desktop/src-tauri/src/keep_awake.rs` moves to
`packages/core/src/keep_awake.rs` unchanged in behaviour: `KeepAwake`,
`InhibitMethod`, `InhibitorSpawner`, `platform_inhibit_method()` and the
`testing` helpers. Linux keeps `systemd-inhibit --what=sleep:idle
--mode=block … sleep infinity`; macOS keeps `caffeinate -i -s -w <pid>`;
Windows keeps `SetThreadExecutionState`.

`CoreState` gains `keep_awake: Arc<KeepAwake>`, created by
`CoreState::new_persistent` with the saved `service_enabled` flag, and by
the test constructors with a recording spawner. The inhibitor is therefore
held by the headless binary too, which is what makes a `systemd --user`
service viable on Omarchy.

Whether the inhibitor is actually held is observable:
`GET /api/status` gains `"sleep_inhibited": bool` (true when
`KeepAwake` has a live inhibitor). `KeepAwake` records whether the last spawn
succeeded; `apply_service_transition` and the new route read it. This is the
seam for the known risk below.

### One service transition, shared by every caller

`apply_service_transition` leaves the Tauri shell and becomes
`CoreState::set_service_enabled_with_effects(enabled) -> bool` (returns the
resulting flag) in `packages/core/src/state.rs`. In order: persist the flag,
broadcast `WsEvent::ServiceStateChanged { enabled }`, then
`keep_awake.set_enabled(enabled)`. The Tauri `set_service_enabled` command
and tray item call it and keep only their own concerns: the tray label and
the `service-changed` window event. The Tauri shell's exit path keeps
releasing the inhibitor through the core's `KeepAwake`.

### New route

`PUT /api/service` with body `{"enabled": bool}` → `200 {"enabled": bool}`.
Paired or loopback, like every non-public route. It calls the shared
transition. Turning the service off stops capture and output work and
releases the inhibitor exactly as the tray does today; turning it on
re-runs the saved restore. `allow_methods` already includes PUT.

### Bluetooth settings handoff on Omarchy

`platform::open_settings` on Linux tries, in order:

1. `omarchy-shell shell toggle omarchy.bluetooth` (Omarchy 4 shell panel),
2. `omarchy-launch-tui bluetui` (Omarchy 3),
3. the existing GNOME, Blueman, KDE candidates.

`Command::spawn` succeeding means the binary exists, so Omarchy 4 never
reaches the `bluetui` candidate and Omarchy 3 never has `omarchy-shell`.

## Terminal UI (`apps/tui`)

### Crate

Workspace member `apps/tui`, crate `on-air-tui`, `[[bin]] name = "on-air"`.
Dependencies: `ratatui`, `crossterm` (with `event-stream`), `tokio`,
`tokio-tungstenite`, `reqwest` (json, rustls), `serde`, `serde_json`,
`futures-util`. It does **not** depend on `on-air-core`: linking the core
would pull cpal, ALSA and the senders into a client binary. Mirrored JSON
types live in `src/api.rs`, named after the TypeScript interfaces in
`packages/api-types`.

Command line: `on-air [--base http://host:port] [--version] [--help]`.
Default base `http://127.0.0.1:47990`. Parsed by hand; no `clap`.

### Modules

- `main.rs`: terminal setup and restore (raw mode, alternate screen, panic
  hook that restores the terminal), the event loop: `tokio::select!` over
  crossterm's `EventStream`, the client's message channel, and a 1 s tick.
- `model.rs`: `Model` (the whole UI state) and the types it is built from:
  `Connection { Connecting, Open, Reconnecting, Unreachable }`,
  `Pane { Sources, Outputs, Mixer }`, `Modal { None, Help, Pin { target,
  input, error } }`, `Pending` (one in-flight action with its label),
  `Snapshot` (status, inputs, active input, outputs, active output, volume,
  EQ, sample rates, CD, PIN, AirPlay mode, Bluetooth devices).
- `update.rs`: `fn update(model: &mut Model, msg: Msg) -> Vec<Cmd>`. Pure:
  no I/O, returns commands. `Msg` covers key presses, every `WsEvent`,
  HTTP results, tick, and connection changes. `Cmd` covers every HTTP call
  (`ActivateInput`, `ActivateOutput`, `DeactivateOutput`, `SetVolume`,
  `SetEq`, `SetSampleRate`, `ControlCd`, `PairAirplay`, `PairBluetooth`,
  `ConnectBluetooth`, `OpenBluetoothSettings`, `SetService`, `Refresh`),
  `SetAutostart`, and `Quit`.
- `view.rs`: `fn view(model: &Model, frame: &mut Frame)`. Layout: header,
  optional CD transport row, optional error banner, three panes side by
  side (Sources, Outputs, Mixer), one-line key hints. Modals draw centered
  over the whole frame.
- `client.rs`: owns `reqwest::Client` and the WebSocket task. Executes
  `Cmd`s, sends `Msg`s back over an `mpsc` channel. Reconnect with jittered
  exponential backoff (500 ms to 10 s) and a 20 s stall timer, the same
  numbers as `@on-air/control-client`. On every (re)connect it refreshes
  the whole snapshot.
- `autostart.rs`: Linux only. `systemctl --user is-enabled on-air-core`,
  `enable --now`, `disable`. On other platforms the key is hidden.
- `keys.rs`: the key map in one table so `view` can render hints from it
  and `update` can match on it.

### Screen and keys

```
 on-air  ● live  Sonos · Living Room        PIN 4821   10.0.0.12   service on
 ▶ Audio CD  03/12  Track title                                     02:14/04:30
 ┌ Sources ───────────┐┌ Outputs ──────────────┐┌ Mixer ──────────────────────┐
 │ > PipeWire monitor │││ Sonos                 ││ Volume   ████████░░  80     │
 │   Built-in mic     ││ > Living Room  ● live  ││ EQ 60   ░░░░█░░░░   +0.0 dB │
 │   Audio CD         ││   Kitchen             ││ EQ 250  ░░░░█░░░░   +0.0 dB │
 │                    ││ AirPlay                ││ EQ 1k   ░░░░██░░░   +2.0 dB │
 │                    ││   HomePod (pair)      ││ EQ 4k   ░░░░█░░░░   +0.0 dB │
 │                    ││ Bluetooth              ││ EQ 12k  ░░░░█░░░░   +0.0 dB │
 │                    ││   JBL Flip  (off)     ││ In 48000  Out 44100         │
 └────────────────────┘└───────────────────────┘└─────────────────────────────┘
  j/k move  Tab pane  Enter select  x stop  +/- adjust  s service  ? help  q quit
```

| Key | Action |
| --- | --- |
| `j`/`k`, `↓`/`↑` | Move within the pane |
| `Tab`, `Shift-Tab`, `h`/`l`, `←`/`→` | Next / previous pane |
| `Enter` | Sources: activate input. Outputs: activate (opens the PIN modal when the device needs pairing). Mixer: no-op |
| `x` | Stop the live output (`DELETE /api/outputs/active`) |
| `+`/`-`, `=`/`_` | Mixer: adjust the selected row (volume ±5, EQ ±1 dB, rates step through the catalog) |
| `0` | Mixer: reset the selected EQ band to 0 dB |
| `Space` | CD play/pause when a disc is present |
| `n`/`p` | CD next / previous track |
| `s` | Service on / off (`PUT /api/service`) |
| `a` | Autostart on / off (Linux: the systemd unit) |
| `b` | Open Bluetooth settings (`POST /api/bluetooth/settings`) |
| `r` | Refresh devices and snapshot |
| `?` | Help modal (the table above) |
| `Esc` | Close modal, dismiss error |
| `q`, `Ctrl-C` | Quit; the service keeps running |

PIN modal: digits only, `Enter` submits, `Esc` cancels; a failed pairing
shows the mapped error under the input and keeps the modal open.

When the service is off, the three panes render dimmed and only `s`, `a`,
`?` and `q` act; the header says `service off — press s`.

When the core is unreachable the header says so and the client keeps
retrying; the last snapshot stays on screen dimmed.

### Theme

The TUI uses only `Color::Reset` for backgrounds and the sixteen named ANSI
colors for foregrounds, plus `BOLD`, `DIM` and `REVERSED` modifiers. No RGB
and no reading of `colors.toml`. Omarchy renders every theme into the
terminal's palette, so the TUI follows the theme the moment the terminal
does, and Hyprland's terminal transparency rule applies because the
background is the terminal's own. Selection is `REVERSED`; live is green;
pending is yellow; errors are red; dimmed states use `DIM`.

### Error copy

`src/error_copy.rs` maps `ApiErrorCode` strings to the same sentences as
`apps/desktop/src/lib/errorCopy.ts`. Unknown codes show the server's
`error` text. A transport that is unreachable shows "Core unreachable on
<base>; is on-air-core running? (systemctl --user status on-air-core)".

## Omarchy packaging and integration

### Files

```
packaging/omarchy/
  on-air-core.service      systemd --user unit
  on-air.desktop           launcher entry (floating terminal)
  on-air.svg               hicolor scalable icon
  on-air-bar               bar status script (POSIX sh, curl)
  snippets/
    omarchy-menu.jsonc     rows for ~/.config/omarchy/extensions/omarchy-menu.jsonc (Omarchy 4)
    bindings.lua           SUPER+SHIFT+O (Omarchy 4)
    bindings.conf          the same binding for Omarchy 3
    shell-bar.json         bar module entry for shell.json (Omarchy 4)
    waybar.jsonc           custom module for Waybar (Omarchy 3)
    hyprland-rules.lua     optional: size rule for the on-air floating window
  install.sh               build + install the package, enable the unit, print snippets
packaging/aur/on-air/PKGBUILD   source build: core + TUI + the files above
```

### Systemd user unit

```ini
[Unit]
Description=on-air core (LAN audio streaming to AirPlay, Bluetooth or Sonos)
After=pipewire.service wireplumber.service pipewire-pulse.service
Wants=pipewire.service

[Service]
Type=simple
ExecStart=/usr/bin/on-air-core
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
```

`default.target` rather than `graphical-session.target`: the service
streams audio and serves the phone whether or not a Hyprland session is
up, and PipeWire itself is a `default.target` user service. Autostart is
`systemctl --user enable on-air-core`; "start now" is `enable --now`.

### Launcher entry

```ini
[Desktop Entry]
Version=1.0
Type=Application
Name=on-air
Comment=LAN audio to AirPlay, Bluetooth or Sonos
Exec=xdg-terminal-exec --app-id=TUI.float -e on-air
Icon=on-air
Terminal=false
Categories=AudioVideo;Audio;
Keywords=airplay;sonos;bluetooth;speaker;
StartupNotify=true
```

This is byte-for-byte the shape `omarchy-tui-install` writes, so the window
gets Omarchy's `floating-window` tag: floating, centered, 875×600, with the
terminal transparency rule. `TUI.float` is used rather than a custom app id
so no Hyprland rule is required; `snippets/hyprland-rules.lua` is offered
for people who want a dedicated size.

### Bar module

`on-air-bar` runs `curl -sf --max-time 1` against `/api/status` and
`/api/outputs/active` on loopback and prints one line:

| State | `text` | `class` |
| --- | --- | --- |
| core down | `󰝛` | `down` |
| service off | `󰝛 off` | `off` |
| no live output | `󰝚` | `idle` |
| live | `󰝚 Living Room` | `live` |

`tooltip` carries transport, device, volume and the LAN address. JSON is
assembled with `printf` and the device name escaped for JSON. Glyphs are
Nerd Font, which Omarchy's bar font has. Omarchy 4 wires it as
`{ "id": "on-air", "type": "command", "exec": "on-air-bar", "interval": 5,
"onClick": "omarchy-launch-or-focus-tui on-air" }`; Omarchy 3 as a Waybar
`custom/on-air` module with `return-type: json`.

### Menu and keybinding snippets

Omarchy 4 menu rows (`~/.config/omarchy/extensions/omarchy-menu.jsonc`):

```jsonc
"on-air": {"icon":"󰝚","label":"on-air"},
"on-air.open": {"icon":"󰝚","label":"Open","action":"omarchy-launch-or-focus-tui on-air"},
"on-air.service": {"icon":"󰐥","label":"Service on/off","action":"on-air-service-toggle"},
"on-air.restart": {"icon":"󰜉","label":"Restart core","action":"systemctl --user restart on-air-core"},
```

`on-air-service-toggle` is a four-line script shipped next to `on-air-bar`
that reads `/api/status` and PUTs the opposite. Keybinding, Omarchy 4
(`~/.config/hypr/bindings.lua`):

```lua
o.bind("SUPER + SHIFT + O", "on-air", { tui = "on-air", focus = true })
```

Omarchy 3 (`~/.config/hypr/bindings.conf`):

```
bindd = SUPER SHIFT, O, on-air, exec, omarchy-launch-or-focus-tui on-air
```

### Arch package

`packaging/aur/on-air/PKGBUILD` builds from the git tag with
`cargo build --frozen --release -p on-air-core -p on-air-tui` and installs:

| File | Destination |
| --- | --- |
| `on-air-core`, `on-air` | `/usr/bin/` |
| `on-air-bar`, `on-air-service-toggle` | `/usr/bin/` |
| `on-air-core.service` | `/usr/lib/systemd/user/` |
| `on-air.desktop` | `/usr/share/applications/` |
| `on-air.svg` | `/usr/share/icons/hicolor/scalable/apps/` |
| snippets | `/usr/share/doc/on-air/omarchy/` |

`depends=('alsa-lib' 'openssl' 'libpulse' 'bluez-utils' 'curl')`
(`libpulse` provides `pactl`, `parec` and `pacat`; `bluez-utils` provides
`bluetoothctl`). `optdepends=('python-pyatv: AirPlay without OwnTone'
'xdg-terminal-exec: launcher entry on non-Omarchy Hyprland')`.
`provides=('on-air' 'on-air-core')`, `conflicts=('on-air-bin'
'on-air-core')`. `check()` runs the workspace tests for both crates.
`.SRCINFO` is generated. `on-air-core` (headless-only) and `on-air-bin`
(webview GUI) stay as they are.

`install.sh` (run from a clone): `makepkg -si` in the recipe directory,
`systemctl --user enable --now on-air-core`, then prints the snippet files
with the path each belongs in. It edits nothing in `~/.config`.

## Release and CI

- `.github/workflows/ci.yml`: the Rust job lints and tests the whole
  workspace minus the Tauri crate on Linux (`-p on-air-core -p on-air-tui`),
  and runs the TUI's mock-core integration test (`apps/tui/tests/mock_core.rs`,
  which spawns the already-built `on-air-core` with `ON_AIR_MOCK=1` on a
  free port, like `e2e/`). The desktop e2e jobs are unchanged.
- `.github/workflows/release.yml`: the Linux matrix entries also build
  `on-air-tui` and stage `on-air-tui-linux-x86_64` / `on-air-tui-linux-aarch64`
  next to the core binaries; `INSTALL.md`'s expected-assets list and
  `packaging/update-checksums.sh`'s missing-file report include them.

## Docs

- `README.md`: the first paragraph says the Linux path is the terminal UI
  over the user service, with Tauri for macOS and Windows.
- `INSTALL.md`: a new first section "Omarchy / Arch (recommended on Linux)"
  with `install.sh`, the manual `makepkg` path, the unit, and a pointer to
  `docs/omarchy.md`. The existing Linux sections stay and gain one line
  saying they install the webview GUI.
- `docs/omarchy.md`: integration guide: launcher, menu, keybinding, bar
  module for both Omarchy versions, idle vs sleep (the shell's idle lock is
  unaffected; suspend is blocked by logind while the service is on), the
  `stay-awake` toggle as a manual fallback, Haswell-era Mac notes from the
  research (nouveau + Intel renderer env for the 2013/2014 15" models,
  `omarchy toggle animations off`), and uninstall.
- `CONTEXT.md`: two definitions: **Control surface** (TUI, Tauri window,
  phone remote: all clients of the same control plane; none owns playback)
  and **Service** (the core process; the thing autostart starts and the
  inhibitor belongs to).
- `apps/tui/README.md`: dev loop (`ON_AIR_MOCK=1 cargo run -p on-air-core`
  in one terminal, `cargo run -p on-air-tui` in another), tests.
- `apps/desktop/README.md` and `packaging/README.md`: one line each on the
  demotion and the new recipe.

## Testing

- Core: unit tests for the shared transition (ported from the Tauri shell's
  `apply_service_transition` tests) and for `KeepAwake` (moved); route
  tests for `PUT /api/service` (toggle, idempotent, 400 on a bad body,
  `/api/status` reflects `service_enabled` and `sleep_inhibited`).
- TUI, pure: `update` tests per key and per `WsEvent` (focus movement,
  activation emits the right `Cmd`, PIN modal flow, service-off lockout,
  unreachable state keeps the snapshot). `view` tests render into
  ratatui's `TestBackend` and assert on the buffer (header states, selection
  marker, dimmed-when-off).
- TUI, integration: `tests/mock_core.rs` runs the golden path against the
  mock core through `client.rs`: snapshot, activate Sonos, volume, EQ, stop,
  service off/on, AirPlay PIN pairing, Bluetooth list, CD simulate.
- Packaging: `packaging/omarchy/test.sh` validates the unit with
  `systemd-analyze verify --user` when available, the desktop file with
  `desktop-file-validate` when available, runs `on-air-bar` against the
  mock core and checks the four JSON states with `jq`, and `sh -n`s the
  scripts. CI runs it on the Linux job.
- Tauri shell: existing tests keep passing with `KeepAwake` imported from
  the core.

## Known risk

logind grants `inhibit-block-sleep` to active sessions. A `systemd --user`
service is not in a session scope; polkit treats such processes as active
when the user has an active session on current polkit versions, but that
has to be confirmed on the laptop. The seam is `sleep_inhibited` in
`/api/status`: if it reports false while the service is on, the TUI header
shows `sleep not inhibited` and `docs/omarchy.md` gives the fallback
(`omarchy-toggle-idle stay-awake`, or a polkit rule for the user).

## Omarchy app conventions this design follows

(Filled from the survey of Omarchy's first-party and community apps; see
the section below.)
