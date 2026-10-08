# on-air — Quality overhaul design

**Date:** 2026-10-07
**Status:** In progress (autonomous session; assumptions are stated, not approved)

## Why

Four parallel audits (Rust core, desktop app, mobile app, shared packages and
tooling) found the same shape of problem in every layer: one god module per
app, hand-copied API plumbing instead of the shared client, correctness bugs
on the main user flows, and a test and CI setup that reports green without
exercising the risky code. The audit notes live outside the repo; the findings
that drive this work are summarised per work package below.

Baseline before any change: `pnpm check:ci`, `pnpm typecheck:web`,
`pnpm test:mobile`, `cargo fmt --check`, `cargo clippy -D warnings` and
`cargo test -p on-air-core` are all green on Linux.

## Goals

1. Fix the user-visible correctness and safety bugs on the pairing, output
   switching, CD, EQ, sleep-inhibit and service-paused paths.
2. Give each app one source of truth for API access (`@on-air/control-client`)
   and for API shapes (`@on-air/api-types`), with the types matching the Rust
   serde structs.
3. Break the three god modules (`CoreState`, desktop `App.tsx`, mobile
   `useRemoteController`) into units with one job each that can be tested in
   isolation.
4. Make CI gate what ships: desktop UI e2e, src-tauri tests, workspace tests on
   every OS, draft-then-publish releases.
5. Remove dead code, scaffold leftovers, personal paths and stale docs.

## Non-goals (deferred, tracked as follow-ups)

- Generating `api-types` from Rust with `ts-rs`. Types are tightened by hand in
  this pass; generation is the next step once the Rust structs settle.
- Replacing `mock: bool` in the core with injected backends. The output and
  input sessions are extracted first; backend injection follows.
- Real mDNS browsing on the phone. The docs and entitlements are corrected to
  describe the /24 sweep that exists; zeroconf is a separate feature.
- Bluetooth over `bluer` D-Bus, login1 D-Bus sleep inhibition, single-instance
  plugin. Each is a dependency decision that deserves its own change.
- Visual redesign. Tokens and a11y are fixed; the look stays.

## Cross-package contracts

These are fixed up front so that agents working in parallel converge.

### Error envelope

Every non-2xx response from the core carries
`{"error": "<human message>", "code": "<snake_case_code>"}` with
`Content-Type: application/json`. Status mapping: 400 validation, 401 not
paired, 404 unknown device or no active output, 409 exclusivity conflict, 429
PIN lockout, 502 transport unreachable, 503 service paused. `HttpError` in the
control client exposes `status`, `body` (raw text) and `code` (parsed when the
body is the envelope). Clients map `code` to copy; they never match on message
text.

### New and changed routes

- `DELETE /api/outputs/active` stops casting. `allow_methods` gains DELETE.
- `GET /api/outputs/active` returns `{"active": null | ActiveOutput}`.
- `POST /api/mock/cd` replaces `POST /api/cd` as the disc simulator and is only
  routed when the core runs in mock mode.
- `GET /api/status` gains `lan_addresses: string[]` so the desktop can show the
  address the phone must type.
- `CdControlRequest.action` is a serde enum; unknown actions are a 400.
- The core sends a WebSocket ping every 10 s.

### Control client surface

Every call accepts a trailing `opts?: { timeoutMs?: number; signal?: AbortSignal }`.
Defaults: 5 s for reads and simple writes; 60 s for `activateOutput`,
`pairAirplay`, `pairBluetooth`, `connectBluetooth`. A timeout rejects with a
`DOMException` named `TimeoutError`. New exports: `getPairingPin`,
`deactivateOutput`, `subscribeEvents(base, token, handlers, opts)` which owns
reconnect with jittered exponential backoff (cap 10 s) and a 20 s stall timer.
`goldenPathSonos`, `switchTransports` move to `e2e/shared/scenarios.ts`;
`prettyInput` moves to `apps/desktop/src/ui/prettyInput.ts`.

### Test IDs

Desktop `data-testid` values used by `e2e/desktop/*.spec.ts` are preserved.
Mobile `testID` values used by `apps/mobile/__tests__` may change together with
the tests; `paired-token` is renamed `paired-host`.

## Work packages

Each package owns a disjoint set of paths. Phases run in order; packages in a
phase run in parallel.

### Phase 1

**WP-A core correctness and API (packages/core)**
- Restore-vs-user TOCTOU: re-check `still_current()` after taking `config_lock`.
- CD re-activation detaches the fresh producer: stop the old capture handle
  before starting the new one; test that PCM still flows after a second
  activation.
- Non-finite EQ gains: 400 in the handler, sanitised in `GraphicEq`.
- `Paired` fails open without `ConnectInfo`: `allow_missing = state.mock`;
  tests inject `ConnectInfo` or use a token.
- Pairing `verify` persists while holding the std mutex every request takes:
  persist outside the lock on a blocking thread.
- Typed errors (`thiserror`) for sender, input and activation paths; one
  `ApiError: IntoResponse` producing the envelope above; delete message-text
  matching.
- Routes above; `__loopback__` becomes a named constant with a comment.
- Per-activation random stream nonce path replaces `stream_generation`.
- Linux-only process helpers (`pactl`, `parec`, `timeout`) behind
  `cfg(target_os = "linux")` with no-op stubs; hard-coded `uv` path from env;
  dev-machine comments removed; unaligned `AudioBufferList` read fixed.
- mDNS: hostname from `gethostname`, bound port threaded in, skipped in mock.
- `parking_lot::Mutex` for the std mutexes; `cfg!(test)` delay becomes config.
- `examples/serve.rs` becomes `src/bin/on-air-core.rs` with `[[bin]]`.
- Dead code listed in the audit removed; shared `net`, `http::read_bounded`,
  `mdns::browse` helpers replace the copy-pasted ones.

**WP-B shared client and types (packages/api-types, packages/control-client)**
- Tighten every drifted field per the audit table; add request types,
  `BluetoothListResponse`, `ActiveOutputResponse`, `ApiErrorBody`, the
  `InputBackend` and `AirPlayMode` unions; `VolumeResponse` renamed with an
  alias; one `MDNS_SERVICE_TYPE` spelling (`_on-air._tcp`).
- Control client surface above; `HttpError.body`/`code`; `apiBase` accepts
  IPv6 literals; `isLanUnicast` bounds octets; `probeOnAir` reads the status
  payload for a name when present.
- Tests for timeout, error body, empty body, caller signal, each untested
  call, and `subscribeEvents` reconnect and stall behaviour.

**WP-D desktop shell (apps/desktop/src-tauri)**
- `KeepAwake` released on the exit path; teardown non-blocking.
- Capabilities trimmed to what the page calls; autostart plugin not registered
  on macOS; opener dependency dropped.
- Autostart defaults off and is not re-enabled on every launch.
- `min_width`/`min_height`, CSP set, manifest metadata filled in.
- `greet`/`get_status` removed; `set_service_enabled` command added for the
  in-window resume button; `autostart_enabled` reads the preference file.
- AirPlay lifecycle test reports skipped, not passed, off macOS.
- `mod` ordering, `block_on` in setup, libdispatch FFI left as-is but
  documented; second-launch path shows the running window where possible
  without a new plugin, otherwise a native dialog.

### Phase 2

**WP-C desktop frontend (apps/desktop/src, apps/desktop/index.html, package.json)**
- Delete `api()`; use the control client and api-types everywhere.
- `useCoreSnapshot` (poll, `subscribeEvents`, apply WS payloads directly,
  debounced split refresh, `null` until loaded, connection state separate from
  action error), `useOutputSelection`, `useAutostart`, `useMixer` with
  latest-write semantics; components `Header`, `StatusPopover`, `DeviceList`,
  `PairDialog` on Radix Dialog, `DeviceHelp`, `MixerFooter`, `PausedPanel`.
- Paused state disables controls and offers resume via the new command.
- Accessibility fixes from the audit; dedupe inputs by name.
- Tokens for every colour and a four-step type scale; one `Button`; dead CSS
  and scaffold files removed; fonts vendored when fetchable, otherwise CSP
  allows the font hosts.
- Vitest and Testing Library tests for the hooks with a fake client and fake
  WebSocket.

**WP-E mobile (apps/mobile)**
- PIN regex; token held in memory before persisting; discovery results never
  discarded; `{host, port}` end-to-end; `AppState` initial value.
- `usePairingStore`, `useDiscovery`, `useDesktopConnection` state machine with
  one coalescing refresh that applies WS payloads via `subscribeEvents`,
  `useLatestWriteQueue`; memoised provider exposing state and actions only;
  `app/index.tsx` becomes a real screen; sheets in `src/sheets/`.
- One `uiState`; one theme in `theme.ts`; shared primitives in `src/ui/`;
  dark-only decision applied; a11y fixes; `NativeFader` ignores props while
  interacting.
- Delete `src/controlClient.ts`, `App.tsx`, `__mocks__/expo.js`; tests use
  `jest.mocked`, open the fake WebSocket, and cover the extracted hooks.
- README claims corrected.

**WP-A2 core structure (packages/core, after WP-A)**
- `session::OutputSession` owning sender, identity, format, nonce and local
  sink under one lock with an explicit state enum and `snapshot()`.
- `pipeline::InputSession` owning capture, active input and supported rates,
  returning `InputError`; processing thread death is reported.
- `CoreState` becomes catalogs, settings and the two sessions; discovery loops
  move next to their parsers.

### Phase 3

**WP-F tooling, CI, e2e, docs (root, .github, e2e, docs, packaging, INSTALL, README)**
- One `typescript` via pnpm `catalog:`; `tsconfig.base.json`; tests included in
  `typecheck`; `turbo.json` graph fixed and used by CI.
- `check-desktop` runs clippy and tests for the workspace on all three OSes;
  mobile e2e in its own job with `extraHosts` and the sweep gated.
- One Playwright config with `api` and `ui` projects, Vite started by the
  config, both in CI; `e2e/shared/{mockCore,scenarios}.ts`; dead runners,
  Detox stub and the npm island removed.
- Release: draft, `needs: publish`, undraft job, version gate covers every
  manifest, recipes use `cargo install --path packages/core`.
- INSTALL/README/RUNNERS corrected; `design-qa.md` moved under `docs/qa/`
  with personal paths removed; `default.nix` source filter.

## Verification

Each agent runs the checks for its area before reporting. After every phase
the coordinator runs the full set: `pnpm check:ci`, `pnpm typecheck:web`,
`pnpm test:mobile`, `cargo fmt --all -- --check`,
`cargo clippy --locked --workspace --all-targets -- -D warnings`,
`cargo test --locked -p on-air-core`, `cargo check -p on-air-desktop`, and the
desktop API e2e and mobile companion e2e against the mock core. macOS and
Windows builds cannot be exercised in this session and are flagged in the PR.
