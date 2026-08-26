# on-air — Design Spec

**Date:** 2026-08-26
**Status:** Approved for planning

## Purpose

A cross-platform application that takes an audio input on a desktop machine and
casts it to wireless outputs — AirPlay, Bluetooth, and Sonos — with basic
signal adjustments (sample rate, EQ, amplifier/gain, volume). It runs as a
background service with a setup GUI, a tray icon, autostart on login, and is
controllable from a companion mobile app over the LAN. There is no WAN/cloud
remote control — control is LAN-only, and the desktop service advertises
itself via mDNS so the mobile app can find it automatically (with a manual-IP
fallback for unreliable networks).

Target desktop platforms: Windows, macOS, and Linux (Omarchy/Arch, Fedora,
Ubuntu). Target mobile platforms: iOS and Android (remote control only).

## Constraints established during research

- **macOS has no public API for programmatic, multi-destination AirPlay
  routing.** `AVRoutePickerView` requires a user click; CoreAudio only shows
  an AirPlay destination after the OS has already routed to it. Any
  app-driven AirPlay send on macOS either goes through a proven third-party
  AirPlay2 sender or resorts to fragile UI-scripting / private API use.
- **Linux has mature, programmable AirPlay2 and multi-room senders**
  (OwnTone/forked-daapd with an HTTP/JSON API; `pyatv` for AirPlay2 +
  HomeKit pairing). This makes Linux the more tractable platform to build
  and validate the sender architecture on first.
- **Sonos does not need AirPlay at all** — it has a fully documented local
  HTTP/UPnP control API on port 1400. This is the lowest-risk, most reliable
  sender to build first.
- **Bluetooth audio output is OS-level device routing, not an app-level
  network protocol.** Once a BT speaker is paired at the OS level, "sending
  audio to it" means directing the PCM stream to that system output sink
  (PipeWire/CoreAudio/WASAPI) — there is no A2DP implementation to write.
- **Latency varies significantly by transport** (~2s AirPlay2 buffer,
  100–300ms Bluetooth, Sonos its own) — any future timing/sync work must be
  verified acoustically (mic loopback), not trusted from reported API
  latency. Out of scope for MVP; noted for later phases.

## Architecture

```
apps/desktop (Tauri)                              apps/mobile (React Native)
┌────────────────────────┐   LAN: mDNS + HTTP/WS   ┌───────────────────────┐
│ React/TS frontend       │◄───────────────────────►│ thin remote client    │
│ (webview)                │                         │ (discovery + manual IP│
└──────────▲───────────────┘                         │  + PIN pairing)       │
           │ Tauri IPC                                └───────────────────────┘
┌──────────┴───────────────┐
│ Rust backend              │
│  = packages/core, linked  │
│    in-process              │
│  owns: input capture, DSP, │
│  sender abstraction, local │
│  HTTP/WS API, mDNS, pairing│
└──────────┬─────────────────┘
           │
   ┌───────┼────────────┐
   ▼                     ▼                     ▼
Sonos                 AirPlay              Bluetooth
(UPnP/SOAP,            (via bundled          (OS output-device
direct)                OwnTone sidecar,      routing — PipeWire/
                        HTTP/JSON)            CoreAudio/WASAPI)
```

### `AudioSender` abstraction

One trait, two implementation shapes:

- **Network-streamed senders** — `SonosSender` (direct UPnP/SOAP),
  `AirPlaySender` (drives a bundled OwnTone subprocess via its HTTP/JSON API
  rather than reimplementing AirPlay2's crypto/streaming in Rust — no mature
  production-grade Rust AirPlay2 crate exists, and OwnTone is a proven
  implementation).
- **OS-output-redirect senders** — `BluetoothSender` points the pipeline's
  output at the paired device's system audio sink instead of streaming over
  a custom protocol.

## `packages/core` (Rust)

- **Input capture**: `cpal` cross-platform, plus platform-specific loopback
  (PipeWire monitor source on Linux; ScreenCaptureKit/BlackHole-style capture
  on macOS; WASAPI loopback on Windows). macOS/Windows loopback work is
  deferred to the M5 milestone under the Linux-first MVP scope.
- **DSP**: biquad EQ, gain/amplifier, `rubato` for sample-rate conversion.
- **Control API**: local `axum` HTTP+WebSocket server. REST for
  configuration (list inputs, list discovered outputs, set volume/EQ/sample
  rate); WebSocket for live level meters and push events (device
  joined/left, sender state changes). Both the Tauri frontend and the mobile
  app are clients of this API — the desktop frontend reaches it over Tauri
  IPC, the mobile app reaches it over the LAN.
- **Discovery**: `mdns-sd` advertising a `_on-air._tcp.local` service
  (hostname, port, version).
- **Pairing**: desktop GUI displays a one-time PIN; mobile exchanges the PIN
  (found via mDNS or entered manual IP — same flow either way) for a
  long-lived token used to authenticate subsequent connections. LAN-only;
  no WAN exposure by design.

## `apps/desktop` (Tauri)

- Rust backend is `packages/core` linked in-process — no separate OS daemon
  for MVP. This is the simplest way to satisfy "runs like a service, with a
  GUI to set things up": the app stays alive in the tray when its window is
  closed, and quitting is an explicit tray-menu action.
- React/TS frontend: input picker, discovered-outputs list with per-output
  volume/EQ/grouping, sample-rate setting, pairing-PIN display, tray-icon
  quick controls (mute, quick output switch, open GUI, quit).
- Autostart via `tauri-plugin-autostart` — login item on macOS/Windows, XDG
  autostart entry on Linux (Omarchy/Fedora/Ubuntu).
- **Deferred, not MVP**: optional systemd `--user` unit on Linux for
  headless daemon operation independent of any GUI session.

## `apps/mobile` (React Native)

- Discovery screen: mDNS-found instances, plus a manual-IP entry fallback
  for unreliable networks — both feed the same PIN-pairing flow.
- Post-pairing screens: source/now-playing info, output selection, per-
  output volume + EQ, using the same shared TS types as the desktop
  frontend.

## Repo layout

```
on-air/
  apps/
    desktop/        # Tauri: Rust backend + React/TS frontend
    mobile/         # React Native
  packages/
    core/           # Rust crate: pipeline, senders, DSP, HTTP/WS API, mDNS, pairing
    api-types/      # shared TS types for desktop frontend + mobile
  turbo.json
  pnpm-workspace.yaml
  Cargo.toml         # workspace root for core
```

Tooling: pnpm workspaces + Turborepo orchestrate the JS/TS side; a Cargo
workspace at the repo root handles the Rust core, wired into Turborepo's task
graph as ordinary tasks.

## Testing strategy

- `packages/core`: Rust unit tests for DSP correctness (biquad filters,
  resampler); integration tests against mocked Sonos/OwnTone HTTP endpoints;
  a `NullSender` (file-sink/no-op) fake sender so pipeline tests run without
  real hardware in CI.
- `apps/desktop` / `apps/mobile`: component-level tests during MVP build-out;
  e2e (Playwright/tauri-driver for desktop, Detox for mobile) deferred until
  the UI stabilizes past MVP — not blocking early milestones.

## MVP milestones (Linux-first)

1. **M0 — Scaffold.** Monorepo wired end-to-end: empty `core` crate serving a
   stub API, a "hello world" Tauri window and RN screen both talking to it.
   Proves the plumbing before any real audio logic exists.
2. **M1 — Linux audio path.** Input capture (PipeWire monitor) → Sonos
   sender → volume/EQ → local HTTP/WS API → mDNS advertisement.
3. **M2 — Desktop GUI.** Wired to `core`; tray icon; autostart verified on
   Ubuntu, Fedora, and Omarchy.
4. **M3 — Mobile remote.** Discovery + manual-IP fallback + PIN pairing +
   basic controls (source, output, volume, EQ) against M1/M2's API.
5. **M4 — AirPlay.** `AirPlaySender` driving the bundled OwnTone sidecar.
6. **M5 — macOS + Windows ports.** Platform-specific loopback capture,
   Bluetooth-as-output-device on each OS, AirPlay caveats on macOS
   (fallback approach if OwnTone doesn't run cleanly there); optional Linux
   systemd `--user` service.

## Explicitly out of scope for MVP

- WAN/cloud remote control (LAN-only by requirement).
- Acoustic latency measurement / cross-device sync (noted as a future
  concern given transport latency differences, not needed until multi-room
  sync is a goal).
- Linux systemd headless-daemon mode (nice-to-have, post-MVP).
- Desktop-to-desktop discovery / multi-room grouping across machines (mDNS
  here is phone-to-desktop only).
