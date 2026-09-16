# On-air desktop icon

The desktop uses the mobile Live Dial identity: an ivory audio dial / lowercase a,
a vermilion live indicator, and the dark console background.

Source: `apps/mobile/assets/icon.png` (1024 × 1024). Regenerate from the repo root:

```sh
pnpm --filter desktop tauri icon ../mobile/assets/icon.png
```

The Tauri generator emits ICNS for macOS, a multi-resolution ICO and Appx tiles
for Windows, and PNG sizes for Linux. This desktop project does not use the
additional generated Android/iOS directories. `tauri.conf.json` selects the bundle
assets; the tray uses Tauri's default window icon. The web preview favicon copies
`32x32.png` to `apps/desktop/public/favicon.png`.

Visual source notes are in `apps/mobile/assets/branding/design-notes.md`.
