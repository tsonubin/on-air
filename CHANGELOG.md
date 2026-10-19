# Changelog

Notable changes to on-air. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Stereo: capture, the Audio CD, the EQ and every speaker stream keep left
  and right apart, so Sonos and HomePod stereo pairs play true stereo. A
  radio stream now uses about 1.5 Mbit/s at 48 kHz.

### Changed

- Repository reorganised for contributors: design records live in
  `docs/design`, the domain glossary in `docs/glossary.md`; added
  `CONTRIBUTING.md`, `SECURITY.md` and this changelog.
- Shared input labels, error copy and test fakes moved into
  `packages/control-client` so the desktop and phone apps no longer carry
  their own diverging copies; the phone no longer drops a second capture
  device whose label matches the first.
- New `on-air-api` crate (`packages/api`) holds the Rust wire types; the
  terminal UI no longer mirrors them by hand.
- The desktop and phone apps share one snapshot engine in
  `packages/control-client`: slice readers, result merging with a staleness
  guard, the refresh verdict, the WebSocket event reducer, the coalescing
  refresh scheduler and the latest-write queue. Both now treat events the
  same way: a `DeviceJoined` shows the speaker at once, an input switch
  re-reads the sample rate, and an output stop for another transport is
  ignored.
- Packaging: one GUI desktop entry, one post-install hook, the systemd
  user unit shipped by the Debian and RPM core packages too; the
  unpublished `on-air-desktop` and `on-air-core` AUR recipes and the
  desktop RPM re-wrap are gone.
- Dependencies: axum 0.8, tower-http 0.7, reqwest 0.13, mdns-sd 0.21,
  sha2 0.11, getrandom 0.4, if-addrs 0.15; Vite 8, Vitest 5, React 19.3
  on the desktop; Jest 30 on the phone; TypeScript 7, Biome 2.5.15,
  Turbo 2.11, Playwright 1.64. Expo SDK 57 pins stay as Expo expects them.

### Fixed

- The phone's pairing screen scrolls up when the keyboard opens, so the
  address and code fields are no longer hidden behind it on iOS or Android.

## [0.2.0] - unreleased

The entries below are grouped by the order the open pull requests are expected
to land, so the section reads as the release will. Nothing here is on `main` yet;
each line names the work, not its PR.

### Added

- Linux is Omarchy-first: `on-air-core` runs as a systemd user service and
  the `on-air` terminal UI (ratatui) controls it, with `bar`, `status`,
  `monitor` and `service` subcommands, mouse support, a Bluetooth device
  list, CD transport and a level meter.
- Arch package and Omarchy snippets (launcher entry, keybinding, menu rows,
  bar module) under `packaging/omarchy`; Flatpak and `~/.local` installers.
- Settings and paired phones persist across core restarts; the core stops
  cleanly on SIGTERM.

#### Wave 1 — core audio stack

- A shared playback timeline: every audio chunk carries a frame index and a
  capture timestamp, so a consumer can say which instant of audio it is
  playing rather than guessing.
- Sync groups. Outputs can be joined and started together at one
  presentation epoch, each holding a delay line so a fast transport waits for
  a slow one, with clock drift steered through a variable-ratio resampler.
  Per-output latency trim and a sync state (`free-running`, `syncing`,
  `locked`, with drift in ppm) are exposed over the API.
- Stereo level metering. The meter reports left and right separately and every
  client draws both, so the stereo this release streams is finally visible.
- DSP quality profiles (`economy`, `balanced`, `high`) selecting the
  resampler, with the audio check recommending one from measurements taken on
  your machine rather than from a guess.
- First-use tuning. A new computer runs the audio check once by itself and
  configures its capture buffer and resampler from the verdict. It never
  overwrites a setting you chose and never runs twice.
- D-Bus MPRIS bridge. External media controls can see and drive on-air. The
  bridge holds no bus connection while nothing is playing, so a machine with
  no disc and no active output shows no phantom player.
- TUI: the audio-check overlay reports what tuning ran and what it chose, and
  the per-output sync state and latency trim.

#### Wave 2 — channel modes and calibration

- Per-output channel mode: `auto`, `stereo` or `mono`. Mono is a genuine
  one-sample-per-frame downmix; stereo on a device that only plays mono is
  refused with an explanation instead of silently dropping a channel.
- Acoustic latency calibration. A probe signal played through a transport and
  cross-correlated against a mic loopback measures that transport's real
  latency, reports its confidence, and replaces the default in the sync
  machinery. Ships as a harness you run on real hardware — **no latency figure
  in this release is measured yet.**
- The help overlay scrolls with the mouse wheel, and a guard asserts that every
  action the help table advertises is reachable from some key.

#### Wave 3 — end-to-end coverage

- e2e suites driving the new API surface through the real client against the
  mock core: sync trims, group lifecycle and its conflict refusals, quality
  selection, first-use tuning, channel modes including their refusal, and the
  error envelope.

#### Wave 4 — Omarchy panel

- The patchbay connects cables only: press a socket and drag to the other
  side. Right-clicking anywhere on a cable removes it immediately. The
  click-to-connect menu is gone.
- Media widget showing the CD transport and the active output, collapsing away
  when nothing is playing.
- Notifications for the events worth knowing: output connected, disconnected
  or failed, audio paused or resumed, and a rotated pairing PIN.

#### Wave 5 — packaging

- One-line Omarchy installer for the prebuilt release, with checksums pinned
  from the release's own `SHA256SUMS` and a refusal to install when they are
  missing or malformed. The AUR recipes follow the submission guidelines.
- Test coverage for the Tauri desktop installer's decision and failure paths.

### Changed

- The source is licensed under MIT.
- L16 conversion rounds to nearest with one LSB of triangular-PDF dither
  instead of truncating toward zero, and scales by 32768 so encode and decode
  are exact inverses. Quiet passages change: truncation was signal-correlated
  distortion.
- The product invariant changes. "Exactly one output at a time" is retired:
  one live sender per session remains, two sessions claiming one receiver is
  still a hard refusal, and simultaneous multi-output is now first-class
  through sync groups.
- TUI keymap: `c` cycles the channel mode, `C` calibrates.

### Fixed

- The desktop installer printed "Installed" and exited 0 when the underlying
  package install had failed.
- The one-line installer could silently fall back to `SKIP` checksums if a
  release recipe's arrays were ever renamed, since `sed` exits 0 whether or not
  it matched. It now refuses to install unverified binaries.
- `bluetoothctl` was called unbounded on the `/api/outputs` request path, which
  hung the request forever on a host without a working system bus.
- The help overlay silently dropped its bottom rows as features accumulated,
  while claiming to fit a 30-row terminal.
- Two features had been bound to the same `c` key, silently disabling one.

## [0.1.0] - 2026-08-27

### Added

- First release: capture desktop audio, stream it to one of Sonos, AirPlay
  or Bluetooth, control it from the Tauri tray app or the Expo phone remote
  over the LAN with PIN pairing.

[Unreleased]: https://github.com/tsonubin/on-air/compare/v0.1.0...HEAD
[0.2.0]: https://github.com/tsonubin/on-air/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tsonubin/on-air/releases/tag/v0.1.0
