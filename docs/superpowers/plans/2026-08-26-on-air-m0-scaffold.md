# on-air M0 Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the on-air monorepo end-to-end — a Rust `core` crate serving a stub status API, a Tauri desktop app that calls it in-process and over HTTP, a React Native mobile app that calls it over the LAN, and a shared TS types package — proving the plumbing before any real audio logic exists.

**Architecture:** pnpm workspaces + Turborepo orchestrate the JS/TS side (`apps/desktop`'s frontend, `apps/mobile`, `packages/api-types`); a Cargo workspace at the repo root holds the Rust side (`packages/core`, `apps/desktop/src-tauri`). `packages/core` exposes both a plain Rust function (`status()`) consumed in-process by the Tauri backend, and an HTTP route (`/api/status`) served on the LAN so the mobile app — which has no Rust dependency — can reach the same data over the network.

**Tech Stack:** Rust (axum 0.7, tokio, serde), Tauri 2.x, React + TypeScript, React Native (bare CLI), pnpm + Turborepo, Cargo workspaces.

**Spec:** `docs/superpowers/specs/2026-08-26-on-air-design.md`

## Global Constraints

- LAN-only control, no WAN/cloud path — anything network-facing binds to the
  local network only, never accepts inbound from the internet. (Spec:
  Purpose)
- `packages/core` is the single source of truth for data both the desktop
  frontend (via Tauri IPC, in-process) and the mobile app (via HTTP over the
  LAN) consume — do not let the two frontends diverge on shape or duplicate
  logic. (Spec: `packages/core` section)
- Tooling is pnpm workspaces + Turborepo for JS/TS, a Cargo workspace for
  Rust, wired so `turbo run <task>` can invoke Cargo tasks like any other
  workspace task. (Spec: Repo layout)
- Repo layout matches the spec exactly: `apps/desktop`, `apps/mobile`,
  `packages/core`, `packages/api-types`, plus root `turbo.json`,
  `pnpm-workspace.yaml`, `Cargo.toml`. (Spec: Repo layout)
- The core HTTP server's default port is `47990` for the whole of M0 — every
  task referencing it (desktop startup, mobile manual-IP entry) must use
  this same constant, not a re-typed literal, once `packages/core` defines
  it in Task 2.

---

### Task 1: `packages/core` — status endpoint

**Files:**
- Create: `pnpm-workspace.yaml`
- Create: `package.json` (repo root)
- Create: `turbo.json`
- Create: `Cargo.toml` (repo root, Cargo workspace)
- Create: `.gitignore`
- Create: `packages/core/Cargo.toml`
- Create: `packages/core/src/lib.rs`
- Test: `packages/core/tests/status_router.rs`

**Interfaces:**
- Consumes: nothing (first task).
- Produces:
  - `pub struct on_air_core::StatusResponse { pub status: &'static str, pub version: &'static str }` (derives `Serialize, PartialEq, Debug`)
  - `pub fn on_air_core::status() -> StatusResponse`
  - `pub fn on_air_core::build_router() -> axum::Router`

- [ ] **Step 1: Create the repo root scaffold**

`pnpm-workspace.yaml`:
```yaml
packages:
  - "apps/*"
  - "packages/*"
```

`package.json`:
```json
{
  "name": "on-air",
  "private": true,
  "scripts": {
    "dev": "turbo run dev",
    "build": "turbo run build",
    "test": "turbo run test",
    "typecheck": "turbo run typecheck"
  },
  "devDependencies": {
    "turbo": "^2.3.0"
  },
  "packageManager": "pnpm@9.15.0"
}
```

`turbo.json`:
```json
{
  "$schema": "https://turbo.build/schema.json",
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**", "target/**"] },
    "dev": { "cache": false, "persistent": true },
    "test": { "dependsOn": ["^build"] },
    "typecheck": { "dependsOn": ["^build"] }
  }
}
```

`Cargo.toml` (repo root):
```toml
[workspace]
resolver = "2"
members = ["packages/core"]
```

`.gitignore`:
```
# Rust
/target
**/target

# Node
node_modules
dist
.turbo

# OS
.DS_Store
```

- [ ] **Step 2: Create the `packages/core` crate skeleton**

`packages/core/Cargo.toml`:
```toml
[package]
name = "on-air-core"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
```

`packages/core/src/lib.rs` (stub for now — just enough to be a valid crate):
```rust
// implemented in the next step
```

- [ ] **Step 3: Write the failing test**

`packages/core/tests/status_router.rs`:
```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn status_route_returns_ok_json() {
    let app = on_air_core::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p on-air-core`
Expected: FAIL to compile — `on_air_core::build_router` does not exist (the crate has no members named `build_router`).

- [ ] **Step 5: Implement `packages/core/src/lib.rs`**

```rust
use axum::{routing::get, Json, Router};
use serde::Serialize;

pub const DEFAULT_PORT: u16 = 47990;

#[derive(Serialize, PartialEq, Debug)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub fn status() -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }
}

pub fn build_router() -> Router {
    Router::new().route("/api/status", get(status_handler))
}

async fn status_handler() -> Json<StatusResponse> {
    Json(status())
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p on-air-core`
Expected: PASS — `status_route_returns_ok_json ... ok`

- [ ] **Step 7: Commit**

```bash
git add pnpm-workspace.yaml package.json turbo.json Cargo.toml .gitignore packages/core
git commit -m "feat(core): scaffold monorepo root and add status endpoint"
```

---

### Task 2: `packages/core` — real HTTP server

**Files:**
- Modify: `packages/core/Cargo.toml`
- Modify: `packages/core/src/lib.rs`
- Test: `packages/core/tests/serve_integration.rs`

**Interfaces:**
- Consumes: `on_air_core::build_router()`, `on_air_core::DEFAULT_PORT` from Task 1.
- Produces:
  - `pub async fn on_air_core::serve(listener: tokio::net::TcpListener) -> std::io::Result<()>`
  - `pub async fn on_air_core::serve_on(addr: std::net::SocketAddr) -> std::io::Result<()>`

- [ ] **Step 1: Add `reqwest` as a dev-dependency**

Add to `packages/core/Cargo.toml` under `[dev-dependencies]`:
```toml
reqwest = { version = "0.12", features = ["json"] }
```

- [ ] **Step 2: Write the failing test**

`packages/core/tests/serve_integration.rs`:
```rust
use tokio::net::TcpListener;

#[tokio::test]
async fn serve_responds_to_status_over_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        on_air_core::serve(listener).await.unwrap();
    });

    let url = format!("http://{addr}/api/status");
    let response = reqwest::get(url).await.unwrap();
    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p on-air-core --test serve_integration`
Expected: FAIL to compile — `on_air_core::serve` does not exist.

- [ ] **Step 4: Implement `serve` and `serve_on`**

Append to `packages/core/src/lib.rs`:
```rust
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, build_router()).await
}

pub async fn serve_on(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p on-air-core`
Expected: PASS — both `status_route_returns_ok_json` and `serve_responds_to_status_over_http` pass.

- [ ] **Step 6: Commit**

```bash
git add packages/core
git commit -m "feat(core): serve the status API over a real HTTP listener"
```

---

### Task 3: `packages/api-types` — shared TS types

**Files:**
- Create: `packages/api-types/package.json`
- Create: `packages/api-types/tsconfig.json`
- Create: `packages/api-types/src/index.ts`

**Interfaces:**
- Consumes: the shape of `on_air_core::StatusResponse` from Task 1 (mirrored by hand — Rust and TS don't share a codegen step in M0).
- Produces: `export interface StatusResponse { status: string; version: string }` from `@on-air/api-types`, imported by Task 4's desktop frontend and Task 5's mobile app.

This package has no runtime logic — it's type-only — so there's no
RED/GREEN test cycle here. Its acceptance criterion is that it type-checks
cleanly and that later tasks can import from it.

- [ ] **Step 1: Create the package**

`packages/api-types/package.json`:
```json
{
  "name": "@on-air/api-types",
  "version": "0.1.0",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts",
  "scripts": {
    "typecheck": "tsc --noEmit"
  },
  "devDependencies": {
    "typescript": "^5.6.0"
  }
}
```

`packages/api-types/tsconfig.json`:
```json
{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "declaration": true,
    "noEmit": true
  },
  "include": ["src"]
}
```

`packages/api-types/src/index.ts`:
```typescript
export interface StatusResponse {
  status: string;
  version: string;
}
```

- [ ] **Step 2: Install and verify it type-checks**

Run: `pnpm install && pnpm --filter @on-air/api-types typecheck`
Expected: exits 0 with no errors.

- [ ] **Step 3: Commit**

```bash
git add packages/api-types pnpm-lock.yaml
git commit -m "feat(api-types): add shared StatusResponse type"
```

---

### Task 4: `apps/desktop` — Tauri shell wired to core

**Files:**
- Create: `apps/desktop/` (via scaffold command below; exact generated paths depend on the installed `create-tauri-app` version — confirm against the generated tree before editing)
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/package.json`
- Modify: `Cargo.toml` (repo root — add `src-tauri` to workspace members)

**Interfaces:**
- Consumes: `on_air_core::status()`, `on_air_core::serve_on()`, `on_air_core::DEFAULT_PORT` (Tasks 1–2); `StatusResponse` from `@on-air/api-types` (Task 3).
- Produces: a running desktop app exposing a Tauri command `get_status` (returns JSON matching `StatusResponse`) and an HTTP status endpoint reachable on the LAN at `http://<host-ip>:47990/api/status`, both consumed by Task 5's mobile app.

- [ ] **Step 1: Scaffold the Tauri app**

Run:
```bash
pnpm dlx create-tauri-app@latest apps/desktop --template react-ts --manager pnpm --yes
```
If the installed CLI rejects these flags, run it without `--yes` and answer the
prompts: app name `desktop`, frontend template `React` + `TypeScript`,
package manager `pnpm`. Inspect the generated tree afterward — Tauri 2.x
scaffolds put the Rust backend in `apps/desktop/src-tauri/` with `main.rs`
calling into a `run()` function in `lib.rs`; if your generated version
differs, apply the edits below to whichever file defines `run()`.

- [ ] **Step 2: Add `packages/core` to the Cargo workspace and as a dependency**

In root `Cargo.toml`, update members:
```toml
[workspace]
resolver = "2"
members = ["packages/core", "apps/desktop/src-tauri"]
```

In `apps/desktop/src-tauri/Cargo.toml`, add under `[dependencies]`:
```toml
on-air-core = { path = "../../../packages/core" }
```

- [ ] **Step 3: Add the `get_status` command and start the HTTP server on launch**

In `apps/desktop/src-tauri/src/lib.rs`, add the command function and wire it
into the builder's `setup` and `invoke_handler`:
```rust
#[tauri::command]
fn get_status() -> on_air_core::StatusResponse {
    on_air_core::status()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            tauri::async_runtime::spawn(async {
                let addr = std::net::SocketAddr::from((
                    [0, 0, 0, 0],
                    on_air_core::DEFAULT_PORT,
                ));
                if let Err(err) = on_air_core::serve_on(addr).await {
                    eprintln!("on-air-core HTTP server failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```
Keep whatever other default-scaffolded builder calls (plugins, etc.) already
present — only add the `.setup(...)` closure and extend
`.invoke_handler(...)` with `get_status`.

- [ ] **Step 4: Add `@on-air/api-types` as a frontend dependency**

In `apps/desktop/package.json`, add under `dependencies`:
```json
"@on-air/api-types": "workspace:*"
```
Run: `pnpm install`

- [ ] **Step 5: Call the command from the frontend**

Replace the default content of `apps/desktop/src/App.tsx` with:
```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StatusResponse } from "@on-air/api-types";

function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);

  useEffect(() => {
    invoke<StatusResponse>("get_status").then(setStatus);
  }, []);

  return (
    <main>
      <h1>on-air</h1>
      <p>
        {status
          ? `core status: ${status.status} (v${status.version})`
          : "loading core status..."}
      </p>
    </main>
  );
}

export default App;
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: exits 0.

Run: `pnpm --filter desktop exec tsc --noEmit`
Expected: exits 0. (If the scaffolded `desktop` package has no bare `tsc`
script, run `pnpm --filter desktop exec tsc --noEmit` directly as shown —
it uses the TypeScript already installed by the scaffold.)

- [ ] **Step 7: Manual verification**

Run: `pnpm --filter desktop tauri dev`
Expected: a window opens showing "core status: ok (v0.1.0)". Leave it
running — Task 5 will hit its HTTP endpoint from a phone/emulator on the
same network to confirm LAN reachability.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml apps/desktop pnpm-lock.yaml
git commit -m "feat(desktop): scaffold Tauri app wired to core status API"
```

---

### Task 5: `apps/mobile` — React Native remote shell

**Files:**
- Create: `apps/mobile/` (via scaffold command below)
- Modify: `apps/mobile/metro.config.js`
- Modify: `apps/mobile/App.tsx`
- Modify: `apps/mobile/package.json`
- Modify: `.gitignore` (append RN-specific ignores)

**Interfaces:**
- Consumes: `StatusResponse` from `@on-air/api-types` (Task 3); the desktop app's HTTP endpoint from Task 4, reached at `http://<manually-entered-ip>:47990/api/status` (mDNS discovery isn't built until milestone M3 — M0 uses manual IP entry only).
- Produces: nothing later tasks depend on — this is the milestone's last deliverable.

- [ ] **Step 1: Scaffold the React Native app**

Run:
```bash
pnpm dlx @react-native-community/cli init OnAirMobile --directory apps/mobile --pm pnpm --skip-install
```
This scaffolds a bare (non-Expo) TypeScript RN project — we'll need native
modules later (mDNS discovery, Bluetooth) that Expo's managed workflow
doesn't support without ejecting, so bare is the right default here.

- [ ] **Step 2: Configure Metro for the pnpm monorepo**

pnpm's symlinked `node_modules` needs explicit `watchFolders` and
`nodeModulesPaths` or Metro won't resolve workspace packages like
`@on-air/api-types`. Replace `apps/mobile/metro.config.js` with:
```javascript
const path = require("path");
const { getDefaultConfig, mergeConfig } = require("@react-native/metro-config");

const workspaceRoot = path.resolve(__dirname, "../..");
const projectRoot = __dirname;

const config = {
  watchFolders: [workspaceRoot],
  resolver: {
    nodeModulesPaths: [
      path.resolve(projectRoot, "node_modules"),
      path.resolve(workspaceRoot, "node_modules"),
    ],
  },
};

module.exports = mergeConfig(getDefaultConfig(projectRoot), config);
```

- [ ] **Step 3: Add `@on-air/api-types` as a dependency**

In `apps/mobile/package.json`, add under `dependencies`:
```json
"@on-air/api-types": "workspace:*"
```
Run: `pnpm install`

- [ ] **Step 4: Build the status-check screen**

Replace `apps/mobile/App.tsx` with:
```tsx
import React, { useState } from "react";
import {
  SafeAreaView,
  Text,
  TextInput,
  Button,
  StyleSheet,
} from "react-native";
import type { StatusResponse } from "@on-air/api-types";

function App(): React.JSX.Element {
  const [ip, setIp] = useState("");
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const checkStatus = async () => {
    setError(null);
    setStatus(null);
    try {
      const response = await fetch(`http://${ip}:47990/api/status`);
      const json: StatusResponse = await response.json();
      setStatus(json);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Text style={styles.title}>on-air remote</Text>
      <TextInput
        style={styles.input}
        placeholder="Desktop LAN IP, e.g. 192.168.1.42"
        value={ip}
        onChangeText={setIp}
        autoCapitalize="none"
      />
      <Button title="Check status" onPress={checkStatus} />
      {status && (
        <Text>{`core status: ${status.status} (v${status.version})`}</Text>
      )}
      {error && <Text style={styles.error}>{error}</Text>}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, padding: 16, gap: 12 },
  title: { fontSize: 20, fontWeight: "600" },
  input: { borderWidth: 1, borderColor: "#999", padding: 8, borderRadius: 6 },
  error: { color: "red" },
});

export default App;
```

- [ ] **Step 5: Verify it type-checks**

Run: `pnpm --filter apps/mobile exec tsc --noEmit`
Expected: exits 0.

- [ ] **Step 6: Add RN build artifacts to `.gitignore`**

Append to `.gitignore`:
```
# React Native
apps/mobile/ios/build
apps/mobile/ios/Pods
apps/mobile/android/.gradle
apps/mobile/android/app/build
```

- [ ] **Step 7: Manual verification**

With Task 4's `pnpm --filter desktop tauri dev` still running, note the
desktop machine's LAN IP (e.g. `ifconfig` / `ipconfig`), then run the mobile
app in a simulator/emulator on the same network:
```bash
pnpm --filter apps/mobile ios     # or: pnpm --filter apps/mobile android
```
Expected: entering the desktop's IP and tapping "Check status" shows
"core status: ok (v0.1.0)" — confirming the mobile app reached
`packages/core`'s HTTP API over the LAN, independent of Tauri IPC.

- [ ] **Step 8: Commit**

```bash
git add apps/mobile .gitignore pnpm-lock.yaml
git commit -m "feat(mobile): scaffold RN remote shell wired to core status API"
```

---

## Self-Review Notes

- **Spec coverage**: M0's stated scope ("empty core crate serving a stub
  API, a hello world Tauri window and RN screen both talking to it") is
  covered end-to-end — Tasks 1–2 build the API, Task 3 shares its shape,
  Task 4 proves the in-process (Tauri IPC) path, Task 5 proves the
  over-the-LAN path, matching the spec's split between how the desktop
  frontend and the mobile app each reach `core`.
- **Placeholder scan**: no TBD/TODO markers; every step has runnable
  commands or complete code. The one deliberate deviation from strict
  TDD is Task 3 (a type-only package), called out explicitly rather than
  faked with a contrived test.
- **Type consistency**: `StatusResponse` (Rust: `status: &'static str`,
  `version: &'static str`) and `StatusResponse` (TS: `status: string`,
  `version: string`) are named identically and field-for-field aligned
  everywhere they're used (Tasks 1, 3, 4, 5). `DEFAULT_PORT` (47990) is
  defined once in Task 2 and referenced by the same literal in Task 4's
  Rust setup code and Task 5's mobile fetch URL — flagged in Global
  Constraints so later milestones don't drift from it.
