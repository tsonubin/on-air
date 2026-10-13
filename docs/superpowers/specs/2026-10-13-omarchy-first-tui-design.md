# on-air — Omarchy-first design: terminal UI over a service

**Date:** 2026-10-13
**Status:** Approved in conversation; spec under review

Companion research: the Omarchy repository (`omacom/omarchy`, master, tags
v3.8.4 and v4.0.4), its agent skill `default/agents/skills/omarchy-app`,
the shipped TUI `cliamp` (bjarneo/cliamp), the `omacom-io/omarchy-pkgs`
recipes, and community apps `omash`, `cliamp-oma-plugin`, `omapantry` and
`omarchy-bitwarden`. The last section lists what was copied from them.

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
4. First-class Omarchy integration: Arch package, launcher entry with its
   own window identity, menu rows, a keybinding, a bar status module, and
   theme-following through the terminal palette plus the theme's accent,
   with no theme files of our own.
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
- MPRIS or desktop notifications from the core. Both would give on-air a
  presence in Omarchy's media widget and toast system; they are follow-ups
  once the TUI path is in use.
- Submitting the package to `omacom-io/omarchy-pkgs` (Omarchy's own pacman
  repo). The recipe is written so that submission is a copy; doing it is a
  follow-up.

## Architecture

```
┌──────────────────────────┐   loopback HTTP + WS    ┌──────────────────────┐
│ on-air (TUI, ratatui)    │ ───────────────────────▶│ on-air-core          │
│ apps/tui                 │                         │ systemd --user unit  │
│ runs in a floating       │   systemctl --user      │ holds the sleep      │
│ terminal, no daemon      │ ──────(autostart)──────▶│ inhibitor            │
└──────────────────────────┘                         └──────────┬───────────┘
┌──────────────────────────┐   loopback HTTP (poll)             │ LAN 47990
│ on-air bar / on-air      │ ───────────────────────────────────┤
│ service (CLI subcommands)│                                    ▼
│ Omarchy bar / Waybar     │                        phone remote, Sonos, …
└──────────────────────────┘
```

Three units, each with one job:

- **Core service** (`packages/core`, binary `on-air-core`): unchanged control
  plane, plus ownership of the sleep inhibitor and a route to toggle the
  service. Runs under `systemd --user` on Linux.
- **Terminal UI** (`apps/tui`, binary `on-air`): a loopback client. Holds no
  audio state; everything it shows comes from the core. Exits without
  touching playback.
- **CLI subcommands** of the same binary: `on-air bar` prints one line of
  Waybar-style JSON for the bar, `on-air status [--json]` prints the
  snapshot, `on-air service on|off|toggle` flips the service. They share
  `client.rs` with the TUI, so there is no shell script to keep in step
  with the API and no `curl` or `jq` dependency.

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
succeeded; `set_service_enabled_with_effects` and the status route read it.
On a platform with no inhibit method it is always false. This is the seam
for the known risk below.

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

Command line, parsed by hand (no `clap`), default base
`http://127.0.0.1:47990`:

```
on-air [--base URL]                 the terminal UI
on-air bar [--base URL]             one line of Waybar-style JSON, exit 0
on-air status [--json] [--base URL] human or JSON snapshot
on-air service on|off|toggle        PUT /api/service, prints the result
on-air --version | --help
```

One binary, one package, one `.desktop`, one icon name, all called
`on-air`: Omarchy's own apps follow that rule and so does this one.

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
- `theme.rs`: the accent color. Reads `accent` from Omarchy's
  `colors.toml` (`~/.local/state/omarchy/current/theme/` on Omarchy 4,
  `~/.config/omarchy/current/theme/` on Omarchy 3); re-reads on the 1 s
  tick when the directory's mtime changed, because a theme switch swaps
  the whole directory. Falls back to ANSI cyan when the file is absent or
  unparsable. Never writes anything under `~/.config/omarchy`.
- `cli.rs`: the `bar`, `status` and `service` subcommands, built on
  `client.rs`, printing and exiting.

### Screen and keys

The window Omarchy gives a floating TUI is 875×600 px, about 100×30 cells
in foot at the default font. The layout is designed for that size and
degrades gracefully when tiled: below 80 columns the three panes stack;
below 40×10 the screen shows "resize the window", as cliamp does.

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
  Tab pane  j/k move  Enter select  h/l adjust  +/- volume  s service  ? help  q quit
```

The key layout follows cliamp, the music TUI Omarchy preinstalls, so a
user who knows one knows the other: `Tab` cycles focus, `j`/`k` move,
`h`/`l` adjust the focused setting, `+`/`-` always mean volume, `Space` is
the main transport action, `?` is the help overlay and `q` quits.

| Key | Action |
| --- | --- |
| `Tab`, `Shift-Tab` | Next / previous pane (Sources → Outputs → Mixer) |
| `j`/`k`, `↓`/`↑` | Move within the pane |
| `Enter` | Sources: activate input. Outputs: activate (opens the PIN modal when the device needs pairing) |
| `h`/`l`, `←`/`→` | Mixer: adjust the focused row (EQ ±1 dB, rates step through the catalog, volume ±5) |
| `+`/`-`, `=`/`_` | Volume ±5 from any pane |
| `0` | Mixer: reset the focused EQ band to 0 dB |
| `x` | Stop the live output (`DELETE /api/outputs/active`) |
| `Space` | CD play/pause when a disc is present |
| `>`/`<`, `.`/`,` | CD next / previous track |
| `s` | Service on / off (`PUT /api/service`) |
| `a` | Autostart on / off (Linux: the systemd unit) |
| `b` | Open Bluetooth settings (`POST /api/bluetooth/settings`) |
| `r` | Refresh devices and snapshot |
| `?` | Help overlay listing this table |
| `Esc` | Close overlay or modal, dismiss the error banner |
| `q`, `Ctrl-C` | Quit; the service keeps running |

PIN modal: digits only, `Enter` submits, `Esc` cancels; a failed pairing
shows the mapped error under the input and keeps the modal open.

When the service is off, the three panes render dimmed and only `s`, `a`,
`?` and `q` act; the header says `service off — press s`.

When the core is unreachable the header says so and the client keeps
retrying; the last snapshot stays on screen dimmed.

"Say what's wrong in the window": a missing backend (no `pactl`, no
`bluetoothctl`, no pyatv or OwnTone) or a refused inhibitor is shown as a
one-line notice under the header, not hidden in a log.

### Theme

Backgrounds are `Color::Reset` everywhere, so the terminal's own
background shows through and Omarchy's terminal opacity and "glassy" rules
apply to the on-air window exactly as they do to btop or cliamp.
Foregrounds are the sixteen named ANSI colors plus `BOLD`, `DIM` and
`REVERSED`. Omarchy renders every theme into the terminal palette and
retints running terminals on a theme switch, so the TUI follows the theme
the moment the terminal does.

The one exception is the accent: selection bars and the "live" marker use
the theme's `accent` from `colors.toml` when it exists (see `theme.rs`),
because that is the color Omarchy's own apps key on, and ANSI cyan
otherwise. No other theme file is read and none is written.

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
  on-air.desktop           launcher entry
  on-air.svg               hicolor scalable icon
  snippets/
    omarchy-menu.jsonc     rows for ~/.config/omarchy/extensions/omarchy-menu.jsonc (Omarchy 4)
    bindings.lua           SUPER+SHIFT+ALT+O (Omarchy 4)
    bindings.conf          the same binding for Omarchy 3
    hyprland-rules.lua     float + center + size for org.omarchy.on-air (Omarchy 4)
    hyprland-rules.conf    the same rule for Omarchy 3
    shell-bar.json         bar module entry for shell.json (Omarchy 4)
    waybar.jsonc           custom module for Waybar (Omarchy 3)
  install.sh               build + install the package, enable the unit, print snippets
  test.sh                  validates the files above (see Testing)
packaging/aur/on-air/PKGBUILD   source build: core + TUI + the files above
packaging/aur/on-air/.SRCINFO
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

### Launcher entry and window identity

```ini
[Desktop Entry]
Version=1.0
Type=Application
Name=on-air
GenericName=Speaker streaming
Comment=LAN audio to AirPlay, Bluetooth or Sonos
Exec=xdg-terminal-exec --app-id=org.omarchy.on-air -e on-air
Icon=on-air
Terminal=false
Categories=AudioVideo;Audio;ConsoleOnly;
Keywords=airplay;sonos;bluetooth;speaker;tui;
StartupNotify=true
```

The Wayland app id is `org.omarchy.on-air` from every entry point: this
`.desktop` file, the hotkey (`{ tui = "on-air" }` expands to
`omarchy-launch-tui on-air`, whose default app id is
`org.omarchy.<command>`), and the bar click
(`omarchy-launch-or-focus-tui on-air`). One id is what makes
focus-or-launch work and what lets a Hyprland rule target on-air alone;
the shared `TUI.float` class that `omarchy-tui-install` uses cannot do
either, which is why it is not used here. Omarchy's own `org.omarchy.*`
pattern already gives the window the `terminal` tag (opacity, glass).

Floating is a user choice, as it is for cliamp: `snippets/hyprland-rules.lua`
adds the `floating-window` tag (float, centered, 875×600) and the docs say
where it goes. Without it the window tiles, and the layout handles that.

`xdg-terminal-exec` is a hard dependency of the package: it is what Omarchy
and Hyprland setups use to open a terminal entry with an app id, and it is
in the Arch `extra` repository for everyone else.

### Bar module

`on-air bar` fetches `/api/status` and `/api/outputs/active` on loopback
with a 1 s timeout and prints one line:

| State | `text` | `class` |
| --- | --- | --- |
| core down | `󰝛` | `down` |
| service off | `󰝛 off` | `off` |
| no live output | `󰝚` | `idle` |
| live | `󰝚 Living Room` | `live` |

`tooltip` carries transport, device, volume and the LAN address. Glyphs
are Nerd Font, which Omarchy's bar font has. Omarchy 4 wires it as a
command module in `~/.config/omarchy/shell.json`:

```json
{ "id": "on-air", "type": "command", "exec": "on-air bar", "interval": 5,
  "tooltip": "on-air", "onClick": "omarchy-launch-or-focus-tui on-air" }
```

Omarchy 3 as a Waybar `custom/on-air` module with `return-type: json`,
the same `exec` and `on-click`. A QML bar-widget plugin, the pattern the
community `cliamp-oma-plugin` uses, can be built later on top of
`on-air status --json` without touching the core.

### Menu and keybinding snippets

Omarchy 4 menu rows (`~/.config/omarchy/extensions/omarchy-menu.jsonc`),
using the same `disabled` guard convention as Omarchy's Install rows:

```jsonc
"on-air": {"icon":"󰝚","label":"on-air"},
"on-air.open": {"icon":"󰝚","label":"Open","action":"omarchy-launch-or-focus-tui on-air"},
"on-air.service": {"icon":"󰐥","label":"Service on/off","action":"on-air service toggle"},
"on-air.restart": {"icon":"󰜉","label":"Restart core","action":"systemctl --user restart on-air-core"},
```

Keybinding, Omarchy 4 (`~/.config/hypr/bindings.lua`), in the slot family
Omarchy uses for its music TUI (`SUPER + SHIFT + ALT + M`):

```lua
o.bind("SUPER + SHIFT + ALT + O", "on-air", { tui = "on-air", focus = true })
```

Omarchy 3 (`~/.config/hypr/bindings.conf`):

```
bindd = SUPER SHIFT ALT, O, on-air, exec, omarchy-launch-or-focus-tui on-air
```

### Arch package

`packaging/aur/on-air/PKGBUILD` follows the shape of Omarchy's own Rust
recipe (`herdr` in `omarchy-pkgs`): `makedepends=('cargo')`,
`options=('!debug' '!lto')`, `prepare()` runs `cargo fetch --locked
--target "$CARCH-unknown-linux-gnu"`, `build()` runs `cargo build --frozen
--release -p on-air-core -p on-air-tui`, `check()` runs `cargo test
--frozen` for both crates. It installs:

| File | Destination |
| --- | --- |
| `on-air-core`, `on-air` | `/usr/bin/` |
| `on-air-core.service` | `/usr/lib/systemd/user/` |
| `on-air.desktop` | `/usr/share/applications/` |
| `on-air.svg` | `/usr/share/icons/hicolor/scalable/apps/` |
| snippets | `/usr/share/doc/on-air/omarchy/` |

`depends=('alsa-lib' 'openssl' 'libpulse' 'bluez-utils' 'xdg-terminal-exec'
'hicolor-icon-theme')` (`libpulse` provides `pactl`, `parec` and `pacat`;
`bluez-utils` provides `bluetoothctl`). `optdepends=('python-pyatv:
AirPlay without OwnTone')`. `provides=('on-air' 'on-air-core')`,
`conflicts=('on-air-bin' 'on-air-core')`. `license=('custom:unknown')`
like the existing recipes: the repository has no LICENSE file yet, and
adding one is outside this change. `.SRCINFO` is generated.
`on-air-core` (headless-only) and `on-air-bin` (webview GUI) stay as they
are.

The icon is the shape Omarchy's app skill prescribes: one square SVG,
`viewBox 0 0 64 64`, a rounded square (`rx="14"`) filled with an accent
color, the on-air live-dial mark from the existing desktop icon drawn on
top in white. The same file serves the launcher and the docs.

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
- CLI: `on-air bar` has a unit test per state (down, off, idle, live)
  against a fake client, plus one run against the mock core in
  `tests/mock_core.rs`; `on-air service toggle` is covered there too.
- Packaging: `packaging/omarchy/test.sh` validates the unit with
  `systemd-analyze verify --user` when available, the desktop file with
  `desktop-file-validate` when available, checks that the SVG parses and
  that the JSONC and Lua snippets are syntactically valid (`jq` after
  stripping comments; `luac -p` when available), and `sh -n`s
  `install.sh`. CI runs it on the Linux job.
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

What was borrowed, and from where, so a reviewer can check the source:

| Convention | Source | Where it lands here |
| --- | --- | --- |
| One name for binary, package, `.desktop`, icon and app id | `omarchy-app` skill (`SKILL.md`, `templates.md`) | `on-air` everywhere; app id `org.omarchy.on-air` |
| App id from `omarchy-launch-tui` is `org.omarchy.<command>`; focus-or-launch matches on it | `bin/omarchy-launch-tui`, `bin/omarchy-launch-or-focus-tui` | `.desktop` `Exec`, hotkey, bar click |
| Floating TUI window is 875×600 centered via the `floating-window` tag; `org.omarchy.*` gets the `terminal` tag | `default/hypr/apps/system.lua`, `apps/terminals.lua` | layout sized for ~100×30 cells; `hyprland-rules` snippet |
| Keys: `Tab` focus, `j`/`k` move, `h`/`l` adjust, `+`/`-` volume, `Space` transport, `?` overlay, `q` quit | cliamp `docs/keybindings.md`; `omarchy-app` skill "keyboard first" | key table above |
| Minimum size 40×10 with a resize notice | cliamp layouts | `view.rs` |
| ANSI palette with `Reset` background so retinting and glass apply; accent from `colors.toml` | cliamp "terminal colors" theme, herdr `panel_bg = "reset"`, omash `src/omarchy.rs`, `omarchy-app` skill `Theme` | `view.rs`, `theme.rs` |
| Re-read the theme when `current/theme` is swapped; never write Omarchy config | `omarchy-app` skill `templates.md` | `theme.rs` |
| `.desktop` with `Terminal=false`, `ConsoleOnly`, `Keywords`, `StartupNotify` | cliamp `cliamp.desktop`, `bin/omarchy-tui-install` | `packaging/omarchy/on-air.desktop` |
| Hotkey family `SUPER + SHIFT + ALT + <letter>` for a media TUI, `{ tui = …, focus = true }` | `default/hypr/bindings/applications.lua` (cliamp) | `snippets/bindings.lua` |
| Menu rows keyed by dotted id with Nerd Font glyph; `disabled` guard | `docs/menu.md`, `manual/31-dotfiles.md` | `snippets/omarchy-menu.jsonc` |
| Bar: `type: "command"` module printing Waybar JSON; QML widget polling a `--json` CLI as the richer option | `docs/omarchy-shell.md`, `cliamp-oma-plugin` | `on-air bar`, `on-air status --json` |
| PKGBUILD: `cargo fetch --locked`, `cargo build --frozen`, `!debug !lto`, `install -Dm` into `/usr/bin`, hicolor, licenses | `omarchy-pkgs` `herdr`, `cliamp`, `omacut` recipes | `packaging/aur/on-air/PKGBUILD` |
| Icon: square SVG, `viewBox 0 0 64 64`, rounded square `rx=14` with the mark on top | `omarchy-app` skill | `packaging/omarchy/on-air.svg` |
| systemd user unit shipped by the package, enabled by the user | omash `omash-supervisor.service`; Omarchy's own `default/systemd/user/*` | `on-air-core.service` |
| "Say what's wrong" in the window | `omarchy-app` skill | header notices |

Deliberately not copied: `TUI.float` as the app id (shared class, no
focus-or-launch), `Terminal=true` entries (no app id under the shell
launcher), reading the whole palette from `colors.toml` (the terminal
already carries it), and a Quickshell plugin (a follow-up on top of the
JSON CLI).
