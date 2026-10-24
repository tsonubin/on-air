# on-air

[![CI](https://github.com/tsonubin/on-air/actions/workflows/ci.yml/badge.svg)](https://github.com/tsonubin/on-air/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Stream your computer's audio to AirPlay, Bluetooth or Sonos speakers.
On Omarchy 4, a compact native Quickshell patchbay mixes inputs and routes
audio to groups of local and network outputs. The terminal, tray app and
phone remote retain their single-output controls. Everything stays on your
LAN: there is no account and no cloud.

- **Sources:** System audio (what the computer plays), microphones and USB
  audio interfaces, or an audio CD. On Linux the list comes from PipeWire, so
  ALSA aliases and on-air's own buses never show up.
- **Omarchy panel:** native Qt Quick/QML, system device defaults, input
  mixes, output groups and hanging patch cables. See [the panel guide](apps/quickshell/README.md).
- **Single-output clients:** Sonos (direct UPnP), AirPlay (OwnTone or pyatv on Linux and
  Windows, the system route picker on macOS), Bluetooth A2DP, and on Linux
  this computer's own speakers or headphone jack. Switching outputs tears the
  previous one down first. System audio cannot play on this computer, since
  that would loop it back into itself.
- **Mixer:** volume, a five-band EQ, input and output sample rates, a level
  meter, CD transport.
- **Remote:** the phone app finds the desktop on the LAN, pairs with a PIN
  and gets the same mixer. Pairing survives restarts on both ends.
- **Service:** the core keeps streaming when every UI is closed, starts at
  login, and holds a sleep inhibitor while audio is live.

## Platforms

| Host | What you run | Status |
| --- | --- | --- |
| Linux (Omarchy, Arch, any systemd desktop) | `on-air-core` as a systemd user service; native Quickshell panel on Omarchy 4 or `on-air` terminal UI | Primary target. Sonos, AirPlay and Bluetooth all work. |
| macOS | The tray app | Sonos and Bluetooth work. AirPlay goes through the system route picker and cannot be driven from the phone. |
| Windows | The tray app | Sonos works. Bluetooth device detection still has gaps; see [docs/history](docs/history/2026-09-16-platform-device-support.md). |
| iOS, Android | The Expo remote | Controls whichever host it pairs with. |

## Install

On Omarchy or Arch:

```bash
curl -fsSL https://raw.githubusercontent.com/tsonubin/on-air/main/packaging/omarchy/install.sh | sh
on-air                         # the terminal UI
```

It installs the latest release's prebuilt binaries as a pacman package
(nothing compiles), starts the `on-air-core` service and prints the
Omarchy snippets. Run it again to update. From a clone (needs Rust and
`base-devel`), `packaging/omarchy/install.sh --checkout` builds your
working tree instead, and `sh packaging/omarchy/install-plugin.sh
--enable` adds the optional native Omarchy 4 panel. Prebuilt installs
need a release with the terminal UI binaries (v0.2.0 or newer; v0.1.0
predates them); until one is published, use `--checkout` from a clone.

Every other path, including Homebrew, winget, APT, RPM, Flatpak, Nix and
building from source, is in [INSTALL.md](INSTALL.md).

## Use

On Omarchy 4 the native panel provides routing and device defaults. The terminal
UI also provides EQ, CD transport and sample-rate controls. `?` shows its keys;
`R` restarts the local core service and reconnects the TUI. The full key and
mouse tables, the launcher entry, hotkey, menu rows and bar
module are in [docs/omarchy.md](docs/omarchy.md). From a shell:

```bash
on-air                              # the terminal UI
on-air monitor on|off|toggle        # audio capture and streaming; the service keeps running
on-air service start|stop|restart   # the on-air-core systemd user service
on-air status [--json]              # what is running and where
on-air bar                          # one Waybar-style JSON line for a bar module
```

On macOS and Windows the tray app does the same: closing the window hides
it, the tray menu opens the mixer, turns audio on or off, or quits. Turning
audio off releases the sleep inhibitor but keeps discovery up so the phone
can still find the desktop.

From a phone on the same Wi-Fi, open the remote. It scans the phone's /24
subnet for the desktop, or you type the address; then enter the PIN the
desktop shows.

## How it works

The core is one Rust process. Its original exclusive session captures audio, runs it through EQ and a
rate bridge, publishes PCM on an in-process bus, and owns exactly one
*sender* at a time. Sonos and AirPlay senders point the speaker at a WAV
stream the core serves under a per-activation nonce; the Bluetooth sender
writes to the OS audio sink. A small HTTP and WebSocket control plane on
port 47990 is the only way in: the terminal UI, the tray window and the
phone are all clients of it and own no playback. Remote clients pair with
a PIN and then a bearer token; the local desktop is trusted by origin.

The output session owns readiness, failure reporting and cleanup. Sonos must
begin fetching its stream, and a direct AirPlay helper must advance its
playhead, before the session reports live playback. A helper or PCM worker
that exits later changes the output to failed with a diagnostic shared by
the clients. Direct AirPlay timing/control uses UDP ports 48000–48127, with
a distinct port pair for each concurrent session.

The Linux patchbay adds a separate graph: PipeWire/PulseAudio buses perform
local mixing and fan-out, and each active network sink gets its own capture
and sender session. Network sinks are selectable in system audio settings.
Groups use independent timing; they do not promise synchronized playback.
The shell only controls the graph, so it can close or reload during playback.

```
packages/core        the core: capture, DSP, senders, control plane (binary on-air-core)
packages/api         the control plane's Rust wire types (shared by core and TUI)
packages/api-types   the same types in TypeScript
packages/control-client   HTTP/WebSocket client the desktop and phone share
apps/tui             on-air, the ratatui terminal UI and CLI (Linux)
apps/quickshell      the native Omarchy 4 patchbay widget (Qt Quick/QML)
apps/desktop         the Tauri tray app: React webview + a Rust shell embedding the core
apps/mobile          the Expo phone remote
e2e                  Playwright and node:test suites against the core in mock mode
packaging            installers and distro recipes
docs                 the Omarchy guide, a domain glossary, design records
```

The domain vocabulary (exclusive output, rate bridge, stereo pair, error
envelope) is in [docs/glossary.md](docs/glossary.md); the design records
behind each milestone are in [docs/design](docs/design).

## Develop

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and the checks CI runs.
The short version, from the repository root after `pnpm install`:

```bash
pnpm check:ci && pnpm typecheck && pnpm test
cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace
ON_AIR_MOCK=1 cargo run -p on-air-core --bin on-air-core   # a hardware-free core on :47990
cargo run -p on-air-tui                                   # the terminal UI against it
```

The e2e suites ([e2e/RUNNERS.md](e2e/RUNNERS.md)) start that mock core
themselves, so no audio hardware or speaker is needed to work on any
client.

## License

[MIT](LICENSE).
