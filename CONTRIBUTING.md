# Contributing

Thanks for taking a look. on-air is a small project, so the process is
light: open an issue to discuss anything larger than a bug fix, then send a
pull request against `main`.

## Set up

You need Rust stable, Node 22 and pnpm 9 (`corepack enable` picks the pinned
version up from `package.json`). On Linux the core also needs
`libasound2-dev`, `libssl-dev` and `pkg-config`; the desktop shell needs the
Tauri WebKitGTK packages listed in [INSTALL.md](INSTALL.md#build-from-source).

```bash
pnpm install
```

## Check before you push

CI runs exactly these; running them locally first saves a round trip.

```bash
pnpm check:ci                 # Biome: format, lint, import order (pnpm format fixes)
pnpm typecheck                # every TypeScript package, tests and e2e included
pnpm test                     # control client, desktop (Vitest), mobile (Jest)
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
sh packaging/omarchy/test.sh  # Omarchy unit, launcher entry, snippets, PKGBUILD
pnpm test:mobile:e2e          # phone client against the mock core
pnpm test:desktop:e2e         # Playwright against the mock core (see e2e/RUNNERS.md)
```

None of them need audio hardware or a running desktop app; the e2e suites
start the core in mock mode (`ON_AIR_MOCK=1`) themselves.

## Layout

| Path | What it is |
| --- | --- |
| `packages/core` | The Rust core: capture, DSP, the Sonos/AirPlay/Bluetooth senders, the HTTP/WebSocket control plane. Headless binary `on-air-core`. |
| `apps/tui` | `on-air`, the ratatui terminal UI and CLI for Linux. A client of the core. |
| `apps/desktop` | The Tauri tray app for macOS and Windows: React webview plus a Rust shell that embeds the core. |
| `apps/mobile` | The Expo phone remote. |
| `packages/api` | The control plane's wire types in Rust (`on-air-api`), serialized by the core and read by the TUI. |
| `packages/api-types`, `packages/control-client` | The same types in TypeScript, and the HTTP/WebSocket client both apps use. |
| `e2e` | Playwright and `node --test` suites against the mock core. |
| `packaging` | Installers and recipes; see `packaging/README.md`. |
| `docs/design` | Design records, written before the code. The code is authoritative. |

Rules the tests enforce: nothing under `packages/core/src` except `api/` may
import `crate::api` (`tests/layering.rs`); the TUI client is driven against
the real core in mock mode (`apps/tui/tests/mock_core.rs`); every error code
has user copy (`packages/control-client/src/errorCopy.test.ts`); error codes
are the stable `code` field of the error envelope, never the `error` text
(see [docs/glossary.md](docs/glossary.md)). When a wire type changes, change
`packages/api` and `packages/api-types` together.

## Commits

Use the conventional prefixes already in the history: `feat(core):`,
`fix(tui):`, `docs:`, `chore(deps):`. Keep the subject under 72 characters
and say what changed for a user or a contributor, not how.

## Releases

Bump the version in every manifest together (the release workflow refuses a
tag whose version disagrees with them), add a `CHANGELOG.md` entry, tag
`vX.Y.Z` and push the tag. `packaging/README.md` covers what happens after
the release is published.
