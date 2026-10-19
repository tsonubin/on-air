# Install on-air

on-air captures audio on your computer and sends it to wireless
outputs: AirPlay, Bluetooth, or Sonos — one per session, or several at once
inside a sync group. On Linux the core runs as a
user service controlled by the `on-air` terminal UI; on macOS and Windows
the desktop app runs in the tray. Both start at login and are controllable
from the phone remote on the same LAN. The phone finds the desktop by
probing port **47990** across its own /24 subnet (or you type the address),
control is HTTP/WebSocket on that port, and LAN clients pair with a PIN.

The latest published release is **v0.1.0**. Everything below that names
`0.2.0` assets describes what the next tag will produce; until it exists,
[build from source](#build-from-source) or use an installer that builds
from a clone.

| Package | What it is |
| --- | --- |
| **on-air** (Linux) | Arch package: the `on-air-core` service plus the `on-air` terminal UI. Recommended on Linux. |
| **on-air-desktop** | Tauri 2 tray app for macOS and Windows (Linux bundles exist too). Embeds the core. |
| **on-air-core** | Headless binary. Same control plane, no UI. |

## Omarchy / Arch (recommended on Linux)

The core runs as a systemd user service and the `on-air` terminal UI
controls it: no WebKit, no GPU toolkit, themed by your terminal. From a
clone (needs Rust and `base-devel`):

```bash
git clone https://github.com/tsonubin/on-air.git && cd on-air
packaging/omarchy/install.sh      # makepkg, sudo pacman -U, enable the service, print the snippets
on-air                            # the terminal UI; `on-air --help` for the subcommands
```

It builds your working tree, uncommitted edits included. Rebuilds are
incremental (compiled output lives in `target/omarchy/`); pass `--clean` to
rebuild everything. The launcher entry, the `Super+Shift+Alt+O` hotkey, the
menu rows, the bar module and the sleep notes are in
[docs/omarchy.md](docs/omarchy.md).

The AUR recipes under `packaging/aur/` (`on-air` from source, `on-air-bin`
prebuilt, `on-air-desktop-bin` for the GUI) are not published to the AUR
yet. From the tree: `cd packaging/aur/<package> && makepkg -si`.

## Desktop app on Linux

The WebKitGTK desktop window also runs on Linux for people who want it.
From a clone, `packaging/install.sh` builds it and installs it the way
your system prefers:

```bash
packaging/install.sh            # Arch: a pacman package. Otherwise Flatpak, else ~/.local
packaging/install.sh yay        # build and install a pacman package (on-air-desktop)
packaging/install.sh flatpak    # per-user Flatpak, com.shay.on-air (GNOME 51 runtime)
packaging/install.sh user       # ~/.local/bin and ~/.local/share, no package manager
```

The Flatpak runs the GUI in the sandbox and shells out to the host for
PipeWire, BlueZ and the AirPlay helper. Prebuilt `.deb`, `.rpm` and
`.AppImage` bundles are on the GitHub Release; see below.

## macOS: Homebrew

This repository is a Homebrew tap (`Casks/` and `Formula/` at the root).

```bash
brew tap tsonubin/on-air https://github.com/tsonubin/on-air.git
brew install --cask on-air        # the tray app, from the release .dmg
brew install on-air-core          # the headless core, built from source
```

The cask installs `on-air-desktop.app` (`aarch64` on Apple silicon, `x64`
on Intel). The formula adds a `brew services` entry for the core.

## Windows: winget

From a clone, once the NSIS installer is on the GitHub Release:

```powershell
winget install --manifest packaging/winget
```

After the manifests are accepted into
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs):
`winget install Tsonubin.OnAir`. Manual fallback: the `-setup.exe`, `.msi`
or `-portable.zip` from the release.

## Debian / Ubuntu: APT

Install the desktop `.deb` from the GitHub Release (`amd64` or `arm64`):

```bash
packaging/apt/install.sh          # downloads and apt-installs for this architecture
```

To build a headless `on-air-core` `.deb` from this tree (needs `cargo`,
`debhelper`, `libasound2-dev`, `pkg-config`, `libssl-dev`):

```bash
packaging/apt/build-deb.sh
```

It ships the systemd user unit; enable it with
`systemctl --user enable --now on-air-core`. There is no hosted APT
repository yet.

## Fedora / RHEL: RPM

Install the desktop RPM from the GitHub Release:

```bash
VERSION=0.2.0
sudo dnf install "https://github.com/tsonubin/on-air/releases/download/v${VERSION}/on-air-desktop-${VERSION}-1.x86_64.rpm"   # or -1.aarch64.rpm
```

Portable: the `.AppImage` from the same release, `chmod +x` and run. To
build the headless core with `rpmbuild` from a source tarball in `SOURCES`:
`rpmbuild -ba packaging/rpm/on-air-core.spec`. It ships the systemd user
unit.

## Nix

`flake.nix` builds **on-air-core**:

```bash
nix run github:tsonubin/on-air            # on-air-core
nix build github:tsonubin/on-air          # ./result/bin/on-air-core
nix develop github:tsonubin/on-air        # rustc, cargo, pnpm, Linux GUI deps
nix build .#on-air-core                   # from a local clone
```

A NixOS module is exported as `nixosModules.default`
(`services.on-air-core.enable = true;`, with `port` and `mock` options).
The desktop app is not a Nix package; use the `.AppImage` or `.deb`.

## Headless core

Every package named `on-air-core` installs one binary that listens on all
interfaces, `0.0.0.0:47990`, so phones and Sonos speakers on the LAN can
reach it. `BIND=127.0.0.1` keeps it loopback-only, `PORT` changes the
port, `ON_AIR_MOCK=1` runs it without hardware. Settings and paired
phones persist in `~/.config/on-air-core/` (`$XDG_CONFIG_HOME/on-air-core`
when set, or `ON_AIR_CONFIG_DIR`).

Release binaries: `on-air-core-serve-<os>-<arch>` for macOS, Linux and
Windows, and the Linux terminal UI as `on-air-tui-linux-<arch>` (it talks
to a core on `127.0.0.1:47990`; `--base` for another address).

## GitHub Releases

https://github.com/tsonubin/on-air/releases

| Platform | Arch | File |
| --- | --- | --- |
| macOS | Apple silicon | `on-air-desktop_<v>_aarch64.dmg` |
| macOS | Intel | `on-air-desktop_<v>_x64.dmg` |
| Linux | x86_64 | `on-air-desktop_<v>_amd64.deb`, `on-air-desktop-<v>-1.x86_64.rpm`, `on-air-desktop_<v>_amd64.AppImage` |
| Linux | arm64 | `on-air-desktop_<v>_arm64.deb`, `on-air-desktop-<v>-1.aarch64.rpm`, `on-air-desktop_<v>_aarch64.AppImage` |
| Linux | x86_64, arm64 | `on-air-tui-linux-<arch>`, `on-air-core-serve-linux-<arch>` |
| Windows | x64, arm64 | `on-air-desktop_<v>_<arch>-setup.exe`, `on-air-desktop_<v>_<arch>_en-US.msi`, `on-air-desktop_<v>_<arch>-portable.zip` |

The full asset list and how the recipes pick their checksums up is in
[packaging/README.md](packaging/README.md).

## Build from source

Needs Rust stable, Node 22, pnpm 9 (`corepack enable` reads the pinned
version from `package.json`), and on Linux: `libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`, `libappindicator3-dev`, `librsvg2-dev`, `libasound2-dev`,
`pkg-config`, `libssl-dev`.

```bash
pnpm install
cargo run --locked --release -p on-air-core --bin on-air-core   # headless core
cargo build --locked --release -p on-air-tui                    # target/release/on-air (Linux UI)
pnpm --dir apps/desktop tauri build                             # desktop app bundles
```

Distro packages for the Linux build deps:

```bash
# Debian/Ubuntu
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf libgtk-3-dev libasound2-dev pkg-config libssl-dev
# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg alsa-lib openssl pkgconf rust
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel alsa-lib-devel openssl-devel pkgconf-pkg-config rust cargo
```

## Mobile remote

The iOS/Android app is an Expo project under `apps/mobile`. From this tree:

```bash
pnpm --filter mobile start          # Expo Go / simulators
pnpm --filter mobile ios
pnpm --filter mobile android
```

The phone must be on the same LAN. It probes port `47990` on every address
in its /24 subnet; on other layouts, type the desktop's IP. Pair with the
PIN the desktop shows. Internal iOS TestFlight builds use
`apps/mobile/scripts/publish-testflight.sh`. Android release builds stay
unsigned until a production keystore is configured.

## After install

1. Launch on-air: on Linux the terminal UI (`a` toggles starting the
   service at login), elsewhere the tray icon with autostart in the app.
2. On macOS, pick the AirPlay destination with the system route picker in
   the app; Sonos, Bluetooth, volume and EQ stay phone-controllable.
3. On the phone, open the remote; it finds the desktop or takes its IP.
4. Enter the PIN the desktop shows. Later calls use a bearer token. The
   desktop window itself needs no pairing.
5. Pick **one** output. Switching tears the current one down first.

## Packager map

| Manager | Files | User command |
| --- | --- | --- |
| Omarchy / Arch | `packaging/omarchy/`, `packaging/aur/on-air` | `packaging/omarchy/install.sh` |
| Desktop on Linux | `packaging/install.sh`, `packaging/flatpak/` | `packaging/install.sh [yay\|flatpak\|user]` |
| Homebrew | `Casks/on-air.rb`, `Formula/on-air-core.rb` | `brew install --cask on-air` |
| Nix | `flake.nix`, `packaging/nix/` | `nix run .#on-air-core` |
| APT | `packaging/debian/`, `packaging/apt/` | `packaging/apt/install.sh` |
| AUR (not published yet) | `packaging/aur/{on-air,on-air-bin,on-air-desktop-bin}` | `makepkg -si` |
| RPM | `packaging/rpm/on-air-core.spec` | `dnf install <url>` / `rpmbuild -ba` |
| winget | `packaging/winget/` | `winget install --manifest packaging/winget` |
