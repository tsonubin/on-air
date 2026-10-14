# Packaging on-air

User-facing install steps are in [`INSTALL.md`](../INSTALL.md). This directory
holds distro recipes. Homebrew lives at the **repository root** (`Casks/`,
`Formula/`) so this repo can be tapped directly.

Version is `0.2.0` everywhere, matching
`apps/desktop/src-tauri/tauri.conf.json`; the release workflow refuses a tag
whose version disagrees with the app manifests. Bump the manifests and the
recipes together, then tag `v0.2.x`.

## After a GitHub Release

```bash
packaging/update-checksums.sh 0.2.0
```

It takes the release's `SHA256SUMS` (uploaded by
`.github/workflows/release.yml`) from `$SHA256SUMS_FILE`, `gh`, or a plain
download, and patches:

- `Casks/on-air.rb` (the two macOS `.dmg`s)
- `Formula/on-air-core.rb` (pins the tag's commit)
- `packaging/aur/on-air-bin/PKGBUILD` (core and terminal UI binaries) and its `.SRCINFO`
- `packaging/aur/on-air-desktop-bin/PKGBUILD` (the `.deb`s) and its `.SRCINFO`
- `packaging/winget/Tsonubin.OnAir.installer.yaml`

`on-air-bin` also downloads the unit, launcher entry, icon, license and
snippets from the tag; their checksums are pinned in the recipe and
`sh packaging/omarchy/test.sh` fails if they drift from the repository.
Source-based recipes (Homebrew formula, Nix flake, AUR `on-air` and
`on-air-core`, Debian and RPM `on-air-core`) need no binary hashes.

## Publishing

### Homebrew

The tap **is** this repository:

```text
brew tap tsonubin/on-air https://github.com/tsonubin/on-air.git
```

A dedicated `homebrew-on-air` repo is optional later; copy `Casks/` and
`Formula/` into it if you want `brew tap tsonubin/on-air` without the extra URL.

### Omarchy / AUR (yay)

> **TODO:** AUR publishing is on hold. The recipes below are kept current,
> but nothing is pushed to aur.archlinux.org yet; users install with
> `packaging/omarchy/install.sh`.

`packaging/aur/on-air` is the Omarchy-first package: the core service plus
the terminal UI, built from source, with the files under `packaging/omarchy/`
(unit, launcher entry, icon, snippets). `sh packaging/omarchy/test.sh`
validates them; `packaging/omarchy/install.sh` is the user-facing installer;
[`docs/omarchy.md`](../docs/omarchy.md) is the integration guide. Submitting
the same recipe to `omacom-io/omarchy-pkgs` (Omarchy's own repo, so
`omarchy pkg add on-air` works) is a copy of that directory.

Four AUR packages, one directory each under `packaging/aur/`: `on-air-bin`
(prebuilt, recommended), `on-air` (source), `on-air-core` (headless) and
`on-air-desktop-bin` (the WebKitGTK window). To publish one, with an AUR
account and its SSH key:

```bash
git clone ssh://aur@aur.archlinux.org/on-air-bin.git aur-on-air-bin
cp packaging/aur/on-air-bin/{PKGBUILD,.SRCINFO,on-air.install} aur-on-air-bin/
cd aur-on-air-bin && makepkg --printsrcinfo > .SRCINFO   # on Arch; matches the repo copy
git add -A && git commit -m "on-air-bin 0.2.0" && git push
```

`packaging/aur/srcinfo.sh <dir>` writes the same `.SRCINFO` where
`makepkg` is missing. Publish `on-air-bin` and `on-air-desktop-bin` only
after `packaging/update-checksums.sh` has filled their binary checksums:
until then they are `SKIP`, so the downloads would go unverified.

### winget

Open a PR against [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs)
with `manifests/t/Tsonubin/OnAir/0.2.0/` copied from `packaging/winget/`.
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

`productName` is `on-air-desktop`. CI builds **x86_64 and arm64** for every
desktop OS:

| OS | Formats |
| --- | --- |
| macOS | `.dmg` |
| Windows | NSIS `.exe`, WiX `.msi`, portable `.zip` |
| Linux | `.deb`, `.rpm`, `.AppImage` |

See INSTALL.md for the filename list. If a future Tauri CLI changes the
pattern, update INSTALL.md, the cask URL, the AUR source, the winget
installer URL, and `packaging/update-checksums.sh` together.