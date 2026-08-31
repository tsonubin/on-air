# Install on-air

on-air captures audio on your computer and sends it to **one** wireless output
at a time — AirPlay, Bluetooth, or Sonos. The desktop app runs in the tray,
can start on login, and is controllable from a companion mobile app on the
same LAN. There is no cloud remote: discovery is mDNS (`_on-air._tcp.local.`),
control is HTTP/WebSocket on port **47990**, and LAN clients pair with a PIN.

A tagged GitHub Release (`v*`) ships **installers** for both **x86_64** and
**arm64** — `.dmg` on macOS, `.msi` / NSIS `.exe` / portable `.zip` on
Windows, `.deb` / `.rpm` / `.AppImage` on Linux. GitHub also attaches an
automatic source zip/tarball; that is not the install path.

| Artifact | What it is |
| --- | --- |
| **on-air-desktop** | Tauri 2 tray app (recommended). Embeds `on-air-core`. |
| **on-air-core** | Headless `serve` binary. Same control plane, no GUI. |

Package-manager recipes live in this repository (`Casks/`, `Formula/`,
`flake.nix`, `packaging/`). They pin version **0.1.0**. Binary recipes expect
the GitHub Release asset names listed in [Expected release assets](#expected-release-assets).
SHA-256 values are filled by `packaging/update-checksums.sh` after a release
publishes (the file `SHA256SUMS` is attached to the release by CI).

Until that release exists, or if the GitHub repository is private, use
[Build from source](#build-from-source).

## Desktop GUI

### macOS — Homebrew

This repository is a Homebrew tap (`Casks/` + `Formula/` at the repo root).

```bash
brew tap tsonubin/on-air https://github.com/tsonubin/on-air.git
brew install --cask on-air
```

If the GitHub repo is private, tap with SSH:

```bash
brew tap tsonubin/on-air git@github.com:tsonubin/on-air.git
```

Or install from a local clone:

```bash
brew tap tsonubin/on-air "$(pwd)"
brew install --cask on-air
```

The cask installs `on-air-desktop.app` from the macOS `.dmg`
(`aarch64` on Apple silicon, `x64` on Intel).

### Windows — winget

From a clone of this repo, once the NSIS installer is on the GitHub Release:

```powershell
winget install --manifest packaging/winget
```

After the manifests are accepted into [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs):

```powershell
winget install Tsonubin.OnAir
```

Manual fallback from the GitHub Release:

- NSIS: `on-air-desktop_0.1.0_x64-setup.exe` or `_arm64-setup.exe`
- MSI: `on-air-desktop_0.1.0_x64_en-US.msi` or `_arm64_en-US.msi`
- Portable zip: `on-air-desktop_0.1.0_x64-portable.zip` or `_arm64-portable.zip`

### Debian / Ubuntu — APT

Install the Tauri `.deb` from the GitHub Release (`amd64` or `arm64`):

```bash
# helper (downloads + apt install for the host architecture)
packaging/apt/install.sh

# or by hand — amd64
VERSION=0.1.0
curl -fLO "https://github.com/tsonubin/on-air/releases/download/v${VERSION}/on-air-desktop_${VERSION}_amd64.deb"
sudo apt install "./on-air-desktop_${VERSION}_amd64.deb"

# arm64
curl -fLO "https://github.com/tsonubin/on-air/releases/download/v${VERSION}/on-air-desktop_${VERSION}_arm64.deb"
sudo apt install "./on-air-desktop_${VERSION}_arm64.deb"
```

To build a **headless** `.deb` of `on-air-core` from this tree (needs
`cargo`, `debhelper`, `libasound2-dev`, `pkg-config`, `libssl-dev`):

```bash
packaging/apt/build-deb.sh
```

Debian packaging files are in `packaging/debian/`. This is not an official
Debian/Ubuntu archive; there is no `apt-add-repository` line until a signed
repo is published.

### Arch Linux — yay / AUR

Recipes are under `packaging/aur/`. They are **not** on the public AUR yet;
install from the tree, or copy the directories into an AUR git package.

Desktop GUI (binary, extracts the release `.deb`):

```bash
cd packaging/aur/on-air-bin
makepkg -si
# after an AUR publish:
# yay -S on-air-bin
```

Headless core (builds with `cargo`):

```bash
cd packaging/aur/on-air-core
makepkg -si
# yay -S on-air-core
```

### Fedora / RHEL — RPM

Install the Tauri-produced RPM from the GitHub Release:

```bash
VERSION=0.1.0
# x86_64
sudo dnf install \
  "https://github.com/tsonubin/on-air/releases/download/v${VERSION}/on-air-desktop-${VERSION}-1.x86_64.rpm"
# aarch64
sudo dnf install \
  "https://github.com/tsonubin/on-air/releases/download/v${VERSION}/on-air-desktop-${VERSION}-1.aarch64.rpm"
```

Portable (no package manager): `on-air-desktop_${VERSION}_amd64.AppImage` or
`on-air-desktop_${VERSION}_aarch64.AppImage`, then `chmod +x` and run.

To build **on-air-core** from source with `rpmbuild`:

```bash
# from the repo root, after putting a source tarball in SOURCES
rpmbuild -ba packaging/rpm/on-air-core.spec
```

`packaging/rpm/on-air-desktop.spec` rebuilds the GUI package from the
GitHub `.rpm` (binary spec) for COPR / local `rpmbuild -bb`.

### Nix

`flake.nix` at the repo root builds **on-air-core** from this tree:

```bash
nix run github:tsonubin/on-air            # on-air-core
nix build github:tsonubin/on-air          # ./result/bin/on-air-core
nix develop github:tsonubin/on-air        # rustc, cargo, pnpm, linux GUI deps
```

From a local clone (works while the GitHub repo is private):

```bash
nix build .#on-air-core
nix run .#on-air-core
```

A NixOS module is exported as `nixosModules.default`:

```nix
{
  inputs.on-air.url = "github:tsonubin/on-air";
  outputs = { nixpkgs, on-air, ... }: {
    nixosConfigurations.example = nixpkgs.lib.nixosSystem {
      modules = [
        on-air.nixosModules.default
        { services.on-air-core.enable = true; }
      ];
    };
  };
}
```

The desktop GUI is not a Nix package yet (it needs the Tauri WebKit stack plus
release hashes). Use the `.AppImage` / `.deb` from the GitHub Release, or
build with `pnpm --dir apps/desktop tauri build`.

## Headless core

Same Homebrew tap:

```bash
brew install on-air-core
```

Nix, APT (`on-air-core` .deb), AUR (`on-air-core`), and RPM
(`on-air-core.spec`) all install a binary named `on-air-core` that listens on
`127.0.0.1:47990` by default. Override with `PORT=47990`. Mock senders with
`ON_AIR_MOCK=1`.

Release binaries (when published) are named:

- `on-air-core-serve-macos-aarch64` / `on-air-core-serve-macos-x86_64`
- `on-air-core-serve-linux-x86_64` / `on-air-core-serve-linux-aarch64`
- `on-air-core-serve-windows-x86_64.exe` / `on-air-core-serve-windows-aarch64.exe`

```bash
chmod +x on-air-core-serve-linux-x86_64
./on-air-core-serve-linux-x86_64
```

## GitHub Releases (manual)

https://github.com/tsonubin/on-air/releases

| Platform | Arch | File |
| --- | --- | --- |
| macOS | Apple silicon | `on-air-desktop_0.1.0_aarch64.dmg` |
| macOS | Intel | `on-air-desktop_0.1.0_x64.dmg` |
| Linux | x86_64 | `on-air-desktop_0.1.0_amd64.deb`, `on-air-desktop-0.1.0-1.x86_64.rpm`, `on-air-desktop_0.1.0_amd64.AppImage` |
| Linux | arm64 | `on-air-desktop_0.1.0_arm64.deb`, `on-air-desktop-0.1.0-1.aarch64.rpm`, `on-air-desktop_0.1.0_aarch64.AppImage` |
| Windows | x64 | `on-air-desktop_0.1.0_x64-setup.exe`, `on-air-desktop_0.1.0_x64_en-US.msi`, `on-air-desktop_0.1.0_x64-portable.zip` |
| Windows | arm64 | `on-air-desktop_0.1.0_arm64-setup.exe`, `on-air-desktop_0.1.0_arm64_en-US.msi`, `on-air-desktop_0.1.0_arm64-portable.zip` |

## Build from source

Needs Rust stable, Node 22, pnpm 9.15.0 (`package.json` `packageManager`),
and on Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libappindicator3-dev`,
`librsvg2-dev`, `libasound2-dev`, `pkg-config`.

```bash
pnpm install

# headless core
cargo run -p on-air-core --example serve --release

# desktop GUI
pnpm --dir apps/desktop tauri build
```

Linux extra packages (Debian/Ubuntu):

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf libgtk-3-dev libasound2-dev pkg-config libssl-dev
```

Arch:

```bash
sudo pacman -S webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg \
  alsa-lib openssl pkgconf rust
```

Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel alsa-lib-devel openssl-devel pkgconf-pkg-config rust cargo
```

## Mobile remote

The iOS/Android app is an **Expo** LAN remote (discovery + PIN pairing + the
same mixer controls as the Tauri UI). It is not shipped through App Store /
Play in v0.1.0. From this tree, with the desktop already open:

```bash
pnpm --filter mobile start          # Expo Go / simulators
pnpm --filter mobile ios
pnpm --filter mobile android
pnpm --filter mobile test
pnpm --filter @on-air/control-client test
```

The phone must be on the same LAN. Scan finds a running core on port `47990`;
you can still type the desktop IP. Pair with the PIN on the desktop.

## After install

1. Launch **on-air** (tray icon). Autostart on login is available from the
   desktop app.
2. On macOS, AirPlay destination is chosen with the system route picker in
   the app; Sonos/Bluetooth and volume/EQ remain remote-controllable.
3. From a phone on the same LAN, open the mobile remote. It should find the
   desktop via mDNS; otherwise enter the desktop’s LAN IP.
4. Pair with the PIN shown on the desktop. Subsequent calls use a Bearer
   token. Loopback clients on the desktop host skip pairing unless
   `require_auth` is on.
5. Pick **one** output. Switching AirPlay / Bluetooth / Sonos tears down the
   current sender first.

Default bind for the GUI is all interfaces, port `47990`. The headless
example binds `127.0.0.1` unless you change `PORT`.

## Expected release assets

CI (`.github/workflows/release.yml`) uploads Tauri bundles plus the core
`serve` example. Names match Tauri 2 defaults for `productName`
`on-air-desktop` version `0.1.0`:

```
# macOS
on-air-desktop_0.1.0_aarch64.dmg
on-air-desktop_0.1.0_x64.dmg
# Linux
on-air-desktop_0.1.0_amd64.deb
on-air-desktop_0.1.0_arm64.deb
on-air-desktop-0.1.0-1.x86_64.rpm
on-air-desktop-0.1.0-1.aarch64.rpm
on-air-desktop_0.1.0_amd64.AppImage
on-air-desktop_0.1.0_aarch64.AppImage
# Windows
on-air-desktop_0.1.0_x64-setup.exe
on-air-desktop_0.1.0_arm64-setup.exe
on-air-desktop_0.1.0_x64_en-US.msi
on-air-desktop_0.1.0_arm64_en-US.msi
on-air-desktop_0.1.0_x64-portable.zip
on-air-desktop_0.1.0_arm64-portable.zip
# Headless
on-air-core-serve-macos-aarch64
on-air-core-serve-macos-x86_64
on-air-core-serve-linux-x86_64
on-air-core-serve-linux-aarch64
on-air-core-serve-windows-x86_64.exe
on-air-core-serve-windows-aarch64.exe
SHA256SUMS
```

After a successful Release run:

```bash
packaging/update-checksums.sh 0.1.0
```

That rewrites Homebrew `sha256`, AUR `sha256sums`, winget `InstallerSha256`,
and documents any still-missing files.

## Packager map

| Manager | Files | User command |
| --- | --- | --- |
| Homebrew cask | `Casks/on-air.rb` | `brew install --cask on-air` |
| Homebrew formula | `Formula/on-air-core.rb` | `brew install on-air-core` |
| Nix | `flake.nix`, `packaging/nix/` | `nix run .#on-air-core` |
| APT | `packaging/debian/`, `packaging/apt/` | `packaging/apt/install.sh` |
| Arch / yay | `packaging/aur/on-air-bin`, `packaging/aur/on-air-core` | `makepkg -si` / `yay -S on-air-bin` |
| RPM | `packaging/rpm/*.spec` | `dnf install <url>` / `rpmbuild -ba` |
| winget | `packaging/winget/` | `winget install --manifest packaging/winget` |

See `packaging/README.md` for how to publish the tap, AUR packages, and
winget-pkgs PR.