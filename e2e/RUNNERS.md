# Desktop E2E runners (M5)

Playwright/`tauri-driver` specs live in `e2e/desktop/`.

| Runner | How to run | This host |
|---|---|---|
| Linux | `npx playwright test` (CI: Ubuntu) | Not this Darwin agent |
| macOS | same Playwright suite against mock core | **Ran here** (API + UI) |
| Windows | same Playwright suite on a Windows GHA runner | Not available in this environment — capture under `{SCRATCH}/desktop-e2e-windows.log` |
| tauri-driver | set `TAURI_DRIVER_URL` and run `tauri-driver.spec.ts` | Binary not installed here; spec still executes `runGoldenPath` against mock core |

## Mobile / Expo companion

The phone is a LAN remote for the desktop mixer: discover `:47990`, PIN-pair, then drive source, destination, volume, EQ, sample rates, and AirPlay/Bluetooth pairing.

| Runner | How to run | This host |
|---|---|---|
| Unit (control-client + Expo UI) | `pnpm test:mobile` | Headless |
| Headless E2E (mock core) | `pnpm test:mobile:e2e` | Starts `on-air-core --example serve` with `ON_AIR_MOCK=1` |
| Detox UI | `e2e/mobile/pairing-sonos.e2e.js` (`ios.sim.debug` / `android.emu.debug`) | Simulators typically missing; spec skips without Detox |
