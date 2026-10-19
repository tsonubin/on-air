#!/bin/bash
# Runs inside flatpak-builder, with the repo sources as the working directory.
set -euo pipefail

export PATH="/usr/lib/sdk/node24/bin:/usr/lib/sdk/rust-stable/bin:${PATH}"
export CARGO_HOME="${CARGO_HOME:-/run/build/on-air/cargo}"

# corepack ships with the Node SDK and resolves pnpm without npm, so the
# build stays pnpm-only end to end.
corepack enable --install-directory /run/build/on-air/pnpm-bin
corepack prepare pnpm@9.15.0 --activate
export PATH="/run/build/on-air/pnpm-bin:${PATH}"

pnpm install --frozen-lockfile --filter 'desktop...'
pnpm --filter desktop build
cargo build --locked --release -p on-air-desktop

install -Dm755 target/release/on-air-desktop /app/bin/on-air-desktop-bin
install -Dm755 on-air-desktop.sh /app/bin/on-air-desktop
install -Dm644 com.shay.on-air.desktop /app/share/applications/com.shay.on-air.desktop
install -Dm644 com.shay.on-air.metainfo.xml /app/share/metainfo/com.shay.on-air.metainfo.xml
install -Dm644 apps/desktop/src-tauri/icons/32x32.png /app/share/icons/hicolor/32x32/apps/com.shay.on-air.png
install -Dm644 apps/desktop/src-tauri/icons/128x128.png /app/share/icons/hicolor/128x128/apps/com.shay.on-air.png
install -Dm644 apps/desktop/src-tauri/icons/128x128@2x.png /app/share/icons/hicolor/256x256/apps/com.shay.on-air.png
install -Dm644 apps/desktop/src-tauri/icons/icon.png /app/share/icons/hicolor/512x512/apps/com.shay.on-air.png
