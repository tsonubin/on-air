# Packaging on-air

User-facing install steps are in [`INSTALL.md`](../INSTALL.md). This directory
holds distro recipes. Homebrew lives at the **repository root** (`Casks/`,
`Formula/`) so this repo can be tapped directly.

Version is `0.1.0` everywhere, matching
`apps/desktop/src-tauri/tauri.conf.json`. Bump that file, then the recipes,
then tag `v0.1.x`.

## After a GitHub Release

```bash
packaging/update-checksums.sh 0.1.0
```

Requires `gh` authenticated against `tsonubin/on-air`. The script pulls
`SHA256SUMS` (uploaded by `.github/workflows/release.yml`) and patches:

- `Casks/on-air.rb`
- `packaging/aur/on-air-bin/PKGBUILD` (+ regenerates `.SRCINFO` if `makepkg` exists)
- `packaging/winget/Tsonubin.OnAir.installer.yaml`

Source-based recipes (Homebrew formula, Nix flake, AUR `on-air-core`, Debian
`on-air-core`, RPM `on-air-core`) do not need binary hashes.

## Publishing

### Homebrew

The tap **is** this repository:

```text
brew tap tsonubin/on-air https://github.com/tsonubin/on-air.git
```

A dedicated `homebrew-on-air` repo is optional later; copy `Casks/` and
`Formula/` into it if you want `brew tap tsonubin/on-air` without the extra URL.

### AUR (yay)

Create `ssh://aur@aur.archlinux.org/on-air-bin.git` and
`ssh://aur@aur.archlinux.org/on-air-core.git`, copy the matching
`packaging/aur/<pkg>/` contents (including `.SRCINFO`), and push. Until then
`makepkg -si` from those directories works.

### winget

Open a PR against [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs)
with `manifests/t/Tsonubin/OnAir/0.1.0/` copied from `packaging/winget/`.
Installer URLs must be publicly downloadable; a private GitHub repo will be
rejected.

### Nix

`nix run github:tsonubin/on-air` works once the repo is public. For a private
flake, use a local path or a `git+ssh://` input.

### APT / RPM

There is no hosted package archive yet. Users install the GitHub `.deb` /
`.rpm`. COPR / Launchpad / Packagecloud can consume
`packaging/rpm/on-air-core.spec` and `packaging/debian/` later.

## Expected Tauri 2 asset names

`productName` is `on-air-desktop`. See INSTALL.md for the full list. If a
future Tauri CLI changes the pattern, update INSTALL.md, the cask URL, the
AUR source, the winget installer URL, and `packaging/update-checksums.sh`
together.