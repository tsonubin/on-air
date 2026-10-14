# Packaging on-air

User-facing install steps are in [`INSTALL.md`](../INSTALL.md). This
directory holds the installers and distro recipes. Homebrew lives at the
**repository root** (`Casks/`, `Formula/`) so the repo can be tapped
directly.

| Path | What it is |
| --- | --- |
| `omarchy/` | The Linux package's files: systemd user unit, launcher entry, icon, Omarchy snippets, the `install.sh` that builds a clone with `makepkg`, and `test.sh`, which CI runs. |
| `aur/on-air`, `aur/on-air-bin`, `aur/on-air-desktop-bin` | AUR recipes: core + TUI from source, the same prebuilt, and the GUI from the release `.deb`. `srcinfo.sh` regenerates `.SRCINFO` without `makepkg`. Not published yet. |
| `install.sh`, `flatpak/` | The desktop app on Linux from a clone: a pacman package, a per-user Flatpak (`com.shay.on-air`), or `~/.local`. |
| `debian/`, `apt/` | A native `on-air-core` Debian package and a helper that installs the release `.deb`. |
| `rpm/on-air-core.spec` | The headless core for `rpmbuild` / COPR. The desktop RPM comes straight from the release. |
| `nix/`, `../flake.nix` | `on-air-core` and a NixOS module. |
| `winget/` | The three winget manifests for the Windows installers. |
| `update-checksums.sh` | Fills SHA-256 pins from a published release's `SHA256SUMS`. |

## One version

Every recipe, default and metainfo entry pins the version in
`packages/core/Cargo.toml`; `sh packaging/omarchy/test.sh` fails when one
drifts, and the release workflow refuses a tag whose version disagrees
with the app manifests. Bump them together, then tag `vX.Y.Z`.

The systemd user unit in `omarchy/on-air-core.service` is the one every
Linux package ships (Arch, Debian via `debian/on-air-core.user.service`,
RPM). The GUI desktop entry is `flatpak/com.shay.on-air.desktop`, used by
`install.sh` and the Flatpak; the TUI's entry is `omarchy/on-air.desktop`.

## After a GitHub Release

```bash
packaging/update-checksums.sh 0.2.0
```

It takes the release's `SHA256SUMS` (uploaded by
`.github/workflows/release.yml`) from `$SHA256SUMS_FILE`, `gh`, or a plain
download, and patches:

- `Casks/on-air.rb` (the two macOS `.dmg`s)
- `Formula/on-air-core.rb` (pins the tag's commit)
- `packaging/aur/on-air-bin/PKGBUILD` and its `.SRCINFO` (core and TUI binaries)
- `packaging/aur/on-air-desktop-bin/PKGBUILD` and its `.SRCINFO` (the `.deb`s)
- `packaging/winget/Tsonubin.OnAir.installer.yaml`

`on-air-bin` also downloads the unit, launcher entry, icon, license and
snippets from the tag; their checksums are pinned in the recipe and
`test.sh` fails if they drift from the repository. Source-based recipes
need no binary hashes. Until the script has run, the binary pins are
`SKIP` (AUR), `:no_check` (cask) or zeros (winget), so publish those
recipes only afterwards.

## Expected release assets

CI builds a draft release with the Tauri bundles for both architectures of
every desktop OS, the headless core, the Linux terminal UI and
`SHA256SUMS`, and publishes it only when every platform succeeded. Names
follow Tauri 2 defaults for `productName` `on-air-desktop`:

```
on-air-desktop_<v>_aarch64.dmg            on-air-desktop_<v>_x64.dmg
on-air-desktop_<v>_amd64.deb              on-air-desktop_<v>_arm64.deb
on-air-desktop-<v>-1.x86_64.rpm           on-air-desktop-<v>-1.aarch64.rpm
on-air-desktop_<v>_amd64.AppImage         on-air-desktop_<v>_aarch64.AppImage
on-air-desktop_<v>_x64-setup.exe          on-air-desktop_<v>_arm64-setup.exe
on-air-desktop_<v>_x64_en-US.msi          on-air-desktop_<v>_arm64_en-US.msi
on-air-desktop_<v>_x64-portable.zip       on-air-desktop_<v>_arm64-portable.zip
on-air-core-serve-macos-aarch64           on-air-core-serve-macos-x86_64
on-air-core-serve-linux-x86_64            on-air-core-serve-linux-aarch64
on-air-core-serve-windows-x86_64.exe      on-air-core-serve-windows-aarch64.exe
on-air-tui-linux-x86_64                   on-air-tui-linux-aarch64
SHA256SUMS
```

If a future Tauri CLI changes the pattern, update INSTALL.md, the cask
URL, the AUR sources, the winget installer URLs and
`update-checksums.sh` together.

## Publishing

**Homebrew.** The tap is this repository:
`brew tap tsonubin/on-air https://github.com/tsonubin/on-air.git`. A
dedicated `homebrew-on-air` repo is optional later.

**AUR.** With an AUR account and its SSH key:

```bash
git clone ssh://aur@aur.archlinux.org/on-air-bin.git aur-on-air-bin
cp packaging/aur/on-air-bin/{PKGBUILD,.SRCINFO,on-air.install} aur-on-air-bin/
cd aur-on-air-bin && makepkg --printsrcinfo > .SRCINFO
git add -A && git commit -m "on-air-bin 0.2.0" && git push
```

Submitting `packaging/aur/on-air` to `omacom-io/omarchy-pkgs` (so
`omarchy pkg add on-air` works) is a copy of that directory.

**winget.** Open a PR against
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) with
`manifests/t/Tsonubin/OnAir/<v>/` copied from `packaging/winget/`.

**APT / RPM.** There is no hosted archive yet; users install the release
`.deb` / `.rpm`. COPR, Launchpad or Packagecloud can consume
`packaging/rpm/on-air-core.spec` and `packaging/debian/` later.
