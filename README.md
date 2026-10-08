# on-air

LAN-only desktop audio streaming to **one** of AirPlay, Bluetooth, or Sonos,
with a tray app, login autostart, and a mobile remote on the same network.

**Install:** see [INSTALL.md](INSTALL.md) (Homebrew, winget, APT, AUR/yay, RPM, Nix, or source).

Control plane listens on port `47990`. Pair from the LAN with a PIN. Transports
are exclusive — switching outputs tears down the current sender first.

This is a **pnpm + turbo + Cargo workspace**. Desktop is Tauri/React
(`apps/desktop`); the iOS/Android remote is Expo (`apps/mobile`); they share
`packages/api-types` and `packages/control-client`. End-to-end suites live in
`e2e/` (see [e2e/RUNNERS.md](e2e/RUNNERS.md)). None of the checks below need the
desktop app running; the e2e suites start a mock core themselves.

```bash
pnpm install
pnpm check              # Biome format, lint and import order (CI runs pnpm check:ci)
pnpm typecheck          # every TypeScript package, tests and e2e included
pnpm test               # control client, desktop (Vitest), mobile (Jest), core
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
pnpm test:mobile:e2e    # phone client against the mock core
pnpm test:desktop:e2e   # Playwright: HTTP golden paths and the desktop UI
pnpm --filter mobile start    # Expo remote; scans the LAN or takes the desktop IP
```

Use `pnpm format` to apply Biome formatting across the web, desktop, mobile,
shared-package, and TypeScript E2E sources. Rust is covered by `cargo fmt` and
`cargo clippy`.

## Tray service and sleep behavior

Closing the desktop window hides it to the tray; it does not stop the service.
The tray menu can open the mixer, turn the audio service on or off, or quit.
Turning the service off stops capture/output work and releases the system sleep
assertion, while leaving low-cost LAN discovery and health status online so the
phone can still find the desktop. Turning it back on restores the last source,
destination, volume, EQ, and sample rates as soon as the saved devices are
discoverable.

While the service is on, the desktop asks macOS, Windows, or systemd-based Linux
to prevent system/idle sleep while still allowing the display to turn off.
Laptop firmware and OS lid-close policy can override an application assertion.
For closed-lid use, configure the OS not to suspend; macOS also requires a
[supported closed-display setup](https://support.apple.com/en-us/102282).
