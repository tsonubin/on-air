# on-air desktop

Tauri 2 shell around the Rust core (`packages/core`), with a React 19 + Vite +
Tailwind v4 webview. The page talks to the in-process core over loopback HTTP
and WebSocket through `@on-air/control-client`.

```bash
pnpm --filter desktop dev          # Vite only, on http://127.0.0.1:1420 (needs a core on :47990)
pnpm --filter desktop tauri dev    # full app: Rust shell + Vite
pnpm --filter desktop build        # typecheck + production bundle into dist/
pnpm --filter desktop test         # Vitest + Testing Library (hooks and components)
pnpm --filter desktop typecheck
```

For a core without hardware, run the mock:
`ON_AIR_MOCK=1 PORT=47990 cargo run -p on-air-core --bin on-air-core`.
Browser e2e specs live in `e2e/desktop`.

Layout of `src/`: `hooks/` own state (core snapshot, output selection, mixer,
service, autostart), `components/` compose the screen, `ui/` are presentational
primitives, `lib/` holds the client binding, error copy and the Tauri bridge.
Design tokens and the type scale live in `App.css`; fonts are vendored under
`assets/fonts/` (OFL).
