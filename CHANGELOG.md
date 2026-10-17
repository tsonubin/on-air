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

### Added

- Linux is Omarchy-first: `on-air-core` runs as a systemd user service and
  the `on-air` terminal UI (ratatui) controls it, with `bar`, `status`,
  `monitor` and `service` subcommands, mouse support, a Bluetooth device
  list, CD transport and a level meter.
- Arch package and Omarchy snippets (launcher entry, keybinding, menu rows,
  bar module) under `packaging/omarchy`; Flatpak and `~/.local` installers.
- Settings and paired phones persist across core restarts; the core stops
  cleanly on SIGTERM.

### Changed

- The source is licensed under MIT.

## [0.1.0] - 2026-08-27

### Added

- First release: capture desktop audio, stream it to one of Sonos, AirPlay
  or Bluetooth, control it from the Tauri tray app or the Expo phone remote
  over the LAN with PIN pairing.

[Unreleased]: https://github.com/tsonubin/on-air/compare/v0.1.0...HEAD
[0.2.0]: https://github.com/tsonubin/on-air/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tsonubin/on-air/releases/tag/v0.1.0
