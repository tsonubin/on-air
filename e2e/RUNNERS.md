# Running the end-to-end suites

Everything here runs against the core in mock mode (`ON_AIR_MOCK=1`): fake
inputs, Sonos/AirPlay/Bluetooth outputs and an Audio CD, no audio hardware.
The suites start the core themselves with
`cargo run --locked -p on-air-core --bin on-air-core`, so a Rust toolchain is
needed (on Linux also `libasound2-dev`). Run every command from the repository
root after `pnpm install`.

| Suite | Command | What it covers |
|---|---|---|
| Phone (headless) | `pnpm test:mobile:e2e` | The shared control client as the Expo app uses it: status probe, PIN pairing, source, destination, volume, EQ, sample rates, AirPlay/Bluetooth pairing, discovery of a known desktop address |
| Desktop API | `pnpm --filter @on-air/e2e test:desktop:api` | HTTP golden paths (`e2e/shared/scenarios.ts`) through the control client |
| Desktop UI | `pnpm --filter @on-air/e2e test:desktop:ui` | The desktop frontend in Chromium, served by Vite on `127.0.0.1:1420` |
| Both desktop projects | `pnpm test:desktop:e2e` | `api` then `ui` |
| Typecheck | `pnpm --filter @on-air/e2e typecheck` | Every `e2e/**/*.ts`, fixtures checked against `@on-air/api-types` |

## Before the first desktop run

Install the browser once:

```sh
pnpm --filter @on-air/e2e exec playwright install chromium
# Linux CI images also need system libraries:
pnpm --filter @on-air/e2e exec playwright install --with-deps chromium
```

## Ports

- The desktop config starts the mock core on `127.0.0.1:47990`, the port the
  desktop frontend calls, and refuses to reuse anything already listening
  there: stop a running desktop app or core first.
- Vite runs on `127.0.0.1:1420`; outside CI an already running `pnpm dev:desktop`
  server is reused.
- The phone suites use `47992` (companion) and `47993` (Expo Go pairing).
  `E2E_PORT` overrides the port for a single file.

## Real LAN sweep

The Expo Go pairing test hands the desktop to discovery as a known address,
so it passes on any machine. To also exercise the phone's /24 sweep, run it on
a host with a private IPv4 (10/8, 172.16/12 or 192.168/16):

```sh
E2E_LAN_SWEEP=1 pnpm test:mobile:e2e
```

The mock core then binds `0.0.0.0` for that run.

## Artifacts

Playwright writes traces of failed tests and screenshots under
`e2e/desktop/test-results/` (git-ignored). Open a trace with
`pnpm --filter @on-air/e2e exec playwright show-trace <path>`.

## On real devices

There is no simulator or Tauri WebDriver suite. Hardware checks (real
speakers, AirPlay, Bluetooth, CD drives) are manual; see
`docs/platform-device-support.md`.
