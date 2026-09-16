# on-air remote (Expo)

LAN remote for the desktop mixer. Lives in this pnpm/turbo monorepo next to
`apps/desktop` and talks to `packages/core` over HTTP + WebSocket.

## Features (parity with the Tauri UI)

- Discover a running desktop on the LAN (`_on-air._tcp` / port 47990 scan)
- PIN pair (the code shown on the desktop)
- Native SwiftUI and Material 3 controls from Expo UI, with automatic light/dark appearance
- Securely restore the last paired desktop after an app restart
- Source + destination lists, stereo-pair badge, AirPlay/Bluetooth pairing sheet
- Volume slider and arrow controls, 5-band EQ, input/output sample rates
- Live refresh + WebSocket (`/api/ws?token=`)

## Sound and connection

Sound opens a native navigation stack: Equalizer contains five labeled dB sliders,
and Audio format groups the advanced sample-rate controls. The system Back button
returns to Sound and then the mixer. Connection has a refresh action that keeps
the screen open and preserves pairing during temporary network failures.

The desktop saves pairing-token hashes in `paired-remotes.json` beside its settings.
Update both apps and pair once with the updated desktop; older desktop versions
stored tokens only in memory. Subsequent desktop restarts retain that pairing.
The phone reconnects its live events automatically, with periodic HTTP refresh as
a fallback. A paused service is distinguished from an unreachable desktop.
If the desktop's LAN address changes, use Forget desktop and pair at the new address.

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
pnpm test:mobile          # control-client + Expo UI unit tests
pnpm test:mobile:e2e      # companion + Expo Go LAN discovery/pairing against mock core
```

## Internal TestFlight

Ship a store-signed iOS build to App Store Connect testers (your App Store
Connect users, no Beta App Review):

```bash
./apps/mobile/scripts/publish-testflight.sh
```

That walks through the Apple Developer membership check, an App Store Connect
API key, teammate invites, then `eas build --platform ios --profile production
--auto-submit`. Testers install from the TestFlight app after the build
finishes processing.
