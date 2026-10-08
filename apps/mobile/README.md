# on-air remote (Expo)

LAN remote for the desktop mixer. Lives in this pnpm/turbo monorepo next to
`apps/desktop` and talks to `packages/core` over HTTP + WebSocket.

## Features

- Find a running desktop by scanning the phone's local /24 for the desktop on
  port 47990, or type the address (`host` or `host:port`) under "Set up manually"
- PIN pair with the six-digit code the desktop shows
- Native SwiftUI and Material 3 controls from Expo UI, in a dark-only appearance
- Securely restore the last paired desktop (host, port and token) after an app restart
- Source and speaker sheets, stereo-pair badge, AirPlay PIN and Bluetooth pairing sheet
- Native volume slider, 5-band EQ, input/output sample rates, audio CD transport
- Live updates over the WebSocket (`/api/ws?token=`), with a slow HTTP poll as a fallback

## Sound and connection

Sound opens a native navigation stack: Equalizer contains five labeled dB sliders,
and Audio format groups the advanced sample-rate controls. The system Back button
returns to Sound and then the mixer. Connection has a refresh action that keeps
the screen open and preserves pairing during temporary network failures.

The desktop saves pairing-token hashes in `paired-remotes.json` beside its settings.
Update both apps and pair once with the updated desktop; older desktop versions
stored tokens only in memory. Subsequent desktop restarts retain that pairing.
The phone reconnects its live events automatically and applies event payloads
directly, with a slow HTTP poll as a fallback. A paused service is distinguished
from an unreachable desktop. If this phone cannot write the pairing to its
keychain, the remote still connects for this session and says so.
If the desktop's LAN address changes, use Forget desktop and pair at the new address.

Autostart and the macOS AirPlay route picker stay desktop-only.

## Run with the desktop

From the repo root, with the Tauri app (or `cargo run -p on-air-core`) already listening on `:47990`:

```bash
pnpm install
pnpm --filter mobile start
```

Then open Expo Go on a phone on the same Wi-Fi, or press `i` / `a` for
simulators. Enter the PIN from the desktop ONAIR chrome.

```bash
pnpm test:mobile          # control-client tests + mobile hook and app tests (Jest)
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
