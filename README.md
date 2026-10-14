# on-air

[![CI](https://github.com/tsonubin/on-air/actions/workflows/ci.yml/badge.svg)](https://github.com/tsonubin/on-air/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Stream your computer's audio to **one** wireless speaker at a time, AirPlay,
Bluetooth or Sonos, and control it from a terminal, a tray app or a phone
on the same network. Everything stays on your LAN: there is no account and
no cloud.

- **Sources:** any capture device, the system monitor, or an audio CD.
- **Outputs:** Sonos (direct UPnP), AirPlay (OwnTone or pyatv on Linux and
  Windows, the system route picker on macOS), Bluetooth A2DP. Switching
  outputs tears the previous one down first.
- **Mixer:** volume, a five-band EQ, input and output sample rates, a level
  meter, CD transport.
- **Remote:** the phone app finds the desktop on the LAN, pairs with a PIN
  and gets the same mixer. Pairing survives restarts on both ends.
- **Service:** the core keeps streaming when every UI is closed, starts at
  login, and holds a sleep inhibitor while audio is live.

## Platforms

| Host | What you run | Status |
| --- | --- | --- |
| Linux (Omarchy, Arch, any systemd desktop) | `on-air-core` as a systemd user service, the `on-air` terminal UI | Primary target. Sonos, AirPlay and Bluetooth all work. |
| macOS | The tray app | Sonos and Bluetooth work. AirPlay goes through the system route picker and cannot be driven from the phone. |
| Windows | The tray app | Sonos works. Bluetooth device detection still has gaps; see [docs/history](docs/history/2026-09-16-platform-device-support.md). |
| iOS, Android | The Expo remote | Controls whichever host it pairs with. |

## Install

On Omarchy or Arch, from a clone (needs Rust and `base-devel`):

```bash
git clone https://github.com/tsonubin/on-air.git && cd on-air
packaging/omarchy/install.sh   # builds, installs, enables the service, prints the Omarchy snippets
on-air                         # the terminal UI
```

Every other path, including Homebrew, winget, APT, RPM, Flatpak, Nix and
building from source, is in [INSTALL.md](INSTALL.md).

## Use

On Linux the terminal UI is the control surface. `?` shows the keys; the
full key and mouse tables, the launcher entry, hotkey, menu rows and bar
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

The core is one Rust process. It captures audio, runs it through EQ and a
rate bridge, publishes PCM on an in-process bus, and owns exactly one
*sender* at a time. Sonos and AirPlay senders point the speaker at a WAV
stream the core serves under a per-activation nonce; the Bluetooth sender
writes to the OS audio sink. A small HTTP and WebSocket control plane on
port 47990 is the only way in: the terminal UI, the tray window and the
phone are all clients of it and own no playback. Remote clients pair with
a PIN and then a bearer token; the local desktop is trusted by origin.

```
packages/core        the core: capture, DSP, senders, control plane (binary on-air-core)
packages/api         the control plane's Rust wire types (shared by core and TUI)
packages/api-types   the same types in TypeScript
packages/control-client   HTTP/WebSocket client the desktop and phone share
apps/tui             on-air, the ratatui terminal UI and CLI (Linux)
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
