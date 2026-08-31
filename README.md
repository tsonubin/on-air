# on-air

LAN-only desktop audio streaming to **one** of AirPlay, Bluetooth, or Sonos,
with a tray app, login autostart, and a mobile remote on the same network.

**Install:** see [INSTALL.md](INSTALL.md) (Homebrew, winget, APT, AUR/yay, RPM, Nix, or source).

Control plane listens on port `47990`. Pair from the LAN with a PIN. Transports
are exclusive — switching outputs tears down the current sender first.

This is a **pnpm + turbo + Cargo workspace**. Desktop is Tauri/React
(`apps/desktop`); the iOS/Android remote is Expo (`apps/mobile`); they share
`packages/api-types` and `packages/control-client`. With the desktop open:

```bash
pnpm test
pnpm check              # Biome format, lint, and import checks
pnpm typecheck
pnpm --filter mobile start    # Expo remote; scan LAN or enter the desktop IP
```

Use `pnpm format` to apply Biome formatting across the web, desktop, mobile,
shared-package, and JavaScript/TypeScript E2E sources. Rust remains covered by
`cargo fmt` and `cargo clippy`.

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
