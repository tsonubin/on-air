# on-air — Design Spec

**Date:** 2026-08-26
**Status:** Approved for planning (revised after first review round)

## Purpose

A cross-platform application that takes an audio input on a desktop machine and
casts it to a wireless output — AirPlay, Bluetooth, or Sonos, one at a time —
with basic signal adjustments (sample rate, EQ, amplifier/gain, volume). It
runs as a background service with a setup GUI, a tray icon, autostart on
login, and is controllable from a companion mobile app over the LAN. There is
no WAN/cloud remote control — control is LAN-only, and the desktop service
advertises itself via mDNS so the mobile app can find it automatically (with a
manual-IP fallback for unreliable networks).

Target desktop platforms: Windows, macOS, and Linux (Omarchy/Arch, Fedora,
Ubuntu). Target mobile platforms: iOS and Android (remote control only).

**Transports are mutually exclusive.** Only one output transport (AirPlay,
Bluetooth, or Sonos) is active at a time. Switching transports tears down the
current sender before starting the next one — there is no simultaneous
multi-transport fan-out in this design.

## Constraints established during research

- **AirPlay routing is platform-specific, not one implementation.**
  - *macOS*: use `AVRoutePickerView`, Apple's native, sanctioned route-picker
    UI — it's macOS, so we use the OS-provided mechanism rather than working
    around it. The tradeoff: it's a system UI element the *user* clicks: there
    is no public API to programmatically select an AirPlay destination
    without someone physically interacting with the picker at the Mac. So on
    macOS, AirPlay device *selection* is a local-only action — the mobile
    remote can reflect current AirPlay state but cannot itself switch AirPlay
    routes on macOS. (Volume/EQ/sample-rate and switching to Sonos/Bluetooth
    remain fully remote-controllable regardless of platform.)
  - *Linux*: no OS-native picker exists, but the platform has mature,
    programmable AirPlay2 senders — OwnTone/forked-daapd (HTTP/JSON API) and
    `pyatv` (AirPlay2 streaming + HomeKit pairing). `AirPlaySender` on Linux
    drives a bundled OwnTone subprocess via its HTTP/JSON API, giving full
    remote control including from the mobile app.
  - *Windows*: no OS-native picker and no mature Rust AirPlay2 stack either.
    Default plan is the same OwnTone-sidecar approach as Linux, to be
    confirmed during the M5 port; flagged as an open question, not a blocker
    for earlier milestones.
- **AirPlay2 pairing is required for some receivers** (e.g. HomePod-style
  devices use a HomeKit-style pair-setup/pair-verify PIN handshake before
  they'll accept a stream). This only matters on the OwnTone/pyatv path
  (Linux, and likely Windows) since pyatv already implements that handshake
  and can surface a "enter this device's PIN" step through our own UI. On
  macOS it's handled invisibly inside the OS's AirPlay picker. It is
  unrelated to Sonos.
- **Sonos does not need AirPlay or any pairing step at all** — it has a
  fully documented local HTTP/UPnP control API on port 1400: discover a
  speaker, control it, no PIN. This is the lowest-risk, most reliable sender
  to build first.
- **Bluetooth audio output is OS-level device routing, not an app-level
  network protocol** — once paired, "sending audio to it" means directing
  the PCM stream to that system output sink (PipeWire/CoreAudio/WASAPI), not
  implementing A2DP. But *pairing itself* still needs real per-OS Bluetooth
  stack integration, since the app needs to help the user discover, pair,
  and select a target device (see Bluetooth UI below) rather than only
  relying on already-paired devices.
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
│  ONE active sender at a    │
│  time, local HTTP/WS API,  │
│  mDNS, pairing              │
└──────────┬─────────────────┘
           │  (exclusive — one active)
   ┌───────┼────────────────────┐
   ▼                             ▼                        ▼
Sonos                         AirPlay                  Bluetooth
UPnP/SOAP, direct,            macOS: AVRoutePickerView   Discover/pair/select
no pairing step               (local-switch only)        via BlueZ (Linux),
                               Linux/Win: bundled OwnTone  IOBluetooth (macOS),
                               sidecar, HTTP/JSON,         WinRT Bluetooth (Win);
                               fully remote-controllable,  route to system audio
                               handles AirPlay2 pairing    sink once connected
```

### `AudioSender` abstraction

One trait (`start` / `stop` / `set_volume` / `name`), with the pipeline
holding exactly one active sender at a time — activating a new sender always
stops the previous one first. Three concrete implementations:

- **`SonosSender`** — direct UPnP/SOAP, no pairing.
- **`AirPlaySender`** — platform-dispatched: `AVRoutePickerView`-driven on
  macOS (local-switch-only, no AirPlay2 pairing needed since the OS handles
  it); bundled OwnTone subprocess via HTTP/JSON on Linux and (planned)
  Windows, including surfacing AirPlay2 pairing PINs through our own UI when
  a receiver requires one.
- **`BluetoothSender`** — routes the pipeline's output to a system audio
  sink once a device is connected, but is paired with a real discovery/pairing
  flow (see Bluetooth UI) rather than assuming a device is already paired.

## `packages/core` (Rust)

- **Input capture**: `cpal` cross-platform, plus platform-specific loopback
  (PipeWire monitor source on Linux; ScreenCaptureKit/BlackHole-style capture
  on macOS; WASAPI loopback on Windows). macOS/Windows loopback work is
  deferred to the M5 milestone under the Linux-first MVP scope.
- **DSP**: biquad EQ, gain/amplifier, `rubato` for sample-rate conversion.
- **Control API**: local `axum` HTTP+WebSocket server. REST for
  configuration (list inputs, list/scan for outputs across all three
  transports, activate a transport+device — deactivating whatever was
  previously active — set volume/EQ/sample rate); WebSocket for live level
  meters and push events (device joined/left, sender state changes, pairing
  prompts e.g. "enter AirPlay2 PIN" or "confirm Bluetooth pairing code").
  Both the Tauri frontend and the mobile app are clients of this API — the
  desktop frontend reaches it over Tauri IPC, the mobile app reaches it over
  the LAN. On macOS, AirPlay-specific route changes are excluded from what
  the API can drive remotely, since that step is local-UI-only.
- **Bluetooth stack integration**: `bluer` (BlueZ/D-Bus) on Linux for full
  in-app scan/pair/connect; `IOBluetooth` on macOS and WinRT
  `Windows.Devices.Bluetooth` on Windows for device scan/connect among
  already-OS-paired devices, with a deep link to the OS's native Bluetooth
  settings panel for pairing brand-new devices where in-app pairing is too
  constrained on those platforms. This mirrors the macOS AirPlay pattern:
  full control on Linux, OS-assisted on macOS/Windows.
- **Discovery**: `mdns-sd` advertising a `_on-air._tcp.local` service
  (hostname, port, version).
- **Pairing (app-level, phone-to-desktop)**: desktop GUI displays a one-time
  PIN; mobile exchanges the PIN (found via mDNS or entered manual IP — same
  flow either way) for a long-lived token used to authenticate subsequent
  connections. LAN-only; no WAN exposure by design. (Distinct from
  AirPlay2/Bluetooth device pairing described above.)

## `apps/desktop` (Tauri)

- Rust backend is `packages/core` linked in-process — no separate OS daemon
  for MVP. This is the simplest way to satisfy "runs like a service, with a
  GUI to set things up": the app stays alive in the tray when its window is
  closed, and quitting is an explicit tray-menu action.
- React/TS frontend: input picker, a single active-output selector (Sonos /
  AirPlay / Bluetooth — mutually exclusive, switching one stops the other),
  a **Bluetooth panel** (scan, pair, connect, select-as-target, with a
  settings deep link on macOS/Windows for OS-level pairing), an
  **AirPlay panel** embedding the native `AVRoutePickerView` on macOS or a
  device list from the OwnTone sidecar on Linux/Windows, volume/EQ/sample-rate
  controls, pairing-PIN display for mobile app-level pairing, tray-icon quick
  controls (mute, quick output switch, open GUI, quit).
- Autostart via `tauri-plugin-autostart` — login item on macOS/Windows, XDG
  autostart entry on Linux (Omarchy/Fedora/Ubuntu).
- **Deferred, not MVP**: optional systemd `--user` unit on Linux for
  headless daemon operation independent of any GUI session.

## `apps/mobile` (React Native)

- Discovery screen: mDNS-found instances, plus a manual-IP entry fallback
  for unreliable networks — both feed the same PIN-pairing flow.
- Post-pairing screens: source/now-playing info, active-output selector
  (reflects current transport; can switch to Sonos or Bluetooth remotely,
  can only *view* — not change — AirPlay state when the desktop is on
  macOS), per-output volume + EQ, using the same shared TS types as the
  desktop frontend.

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
  resampler); integration tests against mocked Sonos/OwnTone HTTP endpoints
  and a mocked Bluetooth stack; a `NullSender` (file-sink/no-op) fake sender
  so pipeline tests run without real hardware in CI; tests asserting sender
  exclusivity (activating a new transport always stops the previous one).
- **E2E tests are required for every target platform, not deferred**:
  Playwright/`tauri-driver` for desktop, covering Windows, macOS, and Linux
  (at least one distro in CI, e.g. Ubuntu, with Fedora/Omarchy validated
  manually or via periodic CI given typical GitHub Actions runner
  availability); Detox for mobile, covering iOS and Android. Since CI
  generally can't reach real Sonos/AirPlay/Bluetooth hardware, e2e suites
  drive the app against the same mocked backends used in `core`'s
  integration tests (fake UPnP responder, fake OwnTone instance, fake
  Bluetooth adapter) for the golden-path flows (pick input, activate a
  transport, adjust volume/EQ, pair a mobile client), with a smaller set of
  manual real-hardware checks (real Sonos speaker, real AirPlay2 receiver,
  real Bluetooth speaker) as a release gate rather than a CI gate.

## MVP milestones (Linux-first)

1. **M0 — Scaffold.** Monorepo wired end-to-end: empty `core` crate serving a
   stub API, a "hello world" Tauri window and RN screen both talking to it.
   Proves the plumbing before any real audio logic exists.
2. **M1 — Linux audio path.** Input capture (PipeWire monitor) → Sonos
   sender → volume/EQ → local HTTP/WS API → mDNS advertisement. Sender
   exclusivity enforced in `core` even though only one transport exists yet.
3. **M2 — Desktop GUI.** Wired to `core`; tray icon; autostart verified on
   Ubuntu, Fedora, and Omarchy; first desktop e2e suite (Sonos golden path
   against the mocked UPnP responder).
4. **M3 — Mobile remote.** Discovery + manual-IP fallback + PIN pairing +
   basic controls (source, output, volume, EQ) against M1/M2's API; first
   mobile e2e suite (Detox, iOS + Android) covering pairing + Sonos control.
5. **M4 — AirPlay + Bluetooth on Linux.** `AirPlaySender` driving the bundled
   OwnTone sidecar, including AirPlay2 pairing-PIN UI; `BluetoothSender` with
   full BlueZ-backed scan/pair/connect UI; transport-switching e2e coverage
   (Sonos ↔ AirPlay ↔ Bluetooth).
6. **M5 — macOS + Windows ports.** Platform-specific loopback capture;
   macOS `AVRoutePickerView`-based AirPlay (local-switch-only) and
   IOBluetooth-based Bluetooth panel; Windows OwnTone-based AirPlay (to be
   confirmed) and WinRT-based Bluetooth panel; desktop e2e extended to
   macOS and Windows runners; optional Linux systemd `--user` service.

## Explicitly out of scope for MVP

- Simultaneous multi-transport output (transports are exclusive by design,
  not just an MVP limitation).
- WAN/cloud remote control (LAN-only by requirement).
- Acoustic latency measurement / cross-device sync (noted as a future
  concern given transport latency differences, not needed until multi-room
  sync is a goal).
- Linux systemd headless-daemon mode (nice-to-have, post-MVP).
- Desktop-to-desktop discovery / multi-room grouping across machines (mDNS
  here is phone-to-desktop only).
- Remote (mobile-initiated) AirPlay device switching on macOS — inherent to
  using `AVRoutePickerView`, not a scoping choice we can lift later without
  abandoning the native picker.
