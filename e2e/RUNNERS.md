# Desktop E2E runners (M5)

Playwright/`tauri-driver` specs live in `e2e/desktop/`.

| Runner | How to run | This host |
|---|---|---|
| Linux | `npx playwright test` (CI: Ubuntu) | Not this Darwin agent |
| macOS | same Playwright suite against mock core | **Ran here** (API + UI) |
| Windows | same Playwright suite on a Windows GHA runner | Not available in this environment — capture under `{SCRATCH}/desktop-e2e-windows.log` |
| tauri-driver | set `TAURI_DRIVER_URL` and run `tauri-driver.spec.ts` | Binary not installed here; spec still executes `runGoldenPath` against mock core |

Mobile Detox: `e2e/mobile/.detoxrc.js` defines `ios.sim.debug` and `android.emu.debug`. Simulators are typically missing on this agent; pairing/control is covered by `apps/mobile/src/controlClient.ts` plus mock-core HTTP tests.
