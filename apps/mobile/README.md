# on-air remote (Expo)

LAN remote for the desktop mixer. Lives in this pnpm/turbo monorepo next to
`apps/desktop` and talks to `packages/core` over HTTP + WebSocket.

## Features (parity with the Tauri UI)

- Discover a running desktop on the LAN (`_on-air._tcp` / port 47990 scan)
- PIN pair (the code shown on the desktop)
- Source + destination lists, stereo-pair badge, AirPlay/Bluetooth pairing sheet
- Volume, 5-band EQ, input/output sample rates
- Live refresh + WebSocket (`/api/ws?token=`)

Autostart and the macOS AirPlay route picker stay desktop-only.

## Run with the desktop

From the repo root, with the Tauri app (or `cargo run -p on-air-core --example serve`) already listening on `:47990`:

```bash
pnpm install
pnpm --filter mobile start
```

Then open Expo Go on a phone on the same Wi-Fi, or press `i` / `a` for
simulators. Enter the PIN from the desktop ONAIR chrome.

```bash
pnpm --filter mobile test
pnpm --filter @on-air/control-client test
```
