#!/bin/sh
# Validates the Omarchy packaging files. Run from the repository root.
# Each check prints "ok <file>" or fails the script. Validators that are
# not installed are skipped with a note, except the greps, which always run.
set -eu

dir=packaging/omarchy
snippets=$dir/snippets
fail() { echo "FAIL: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
need() { [ -f "$1" ] || fail "missing $1"; }

for f in "$dir/on-air-core.service" "$dir/on-air.desktop" "$dir/on-air.svg" \
         "$dir/install.sh" "$snippets/omarchy-menu.jsonc" "$snippets/bindings.lua" \
         "$snippets/bindings.conf" "$snippets/hyprland-rules.lua" \
         "$snippets/hyprland-rules.conf" "$snippets/shell-bar.json" \
         "$snippets/waybar.jsonc" packaging/aur/on-air/PKGBUILD \
         packaging/aur/on-air/.SRCINFO; do
  need "$f"
done

# systemd user unit
if have systemd-analyze; then
  # Prefer the user manager; without a user session (CI, containers) the
  # system manager parses the same directives.
  out=$(systemd-analyze --user verify "$dir/on-air-core.service" 2>&1 || true)
  case "$out" in
    *"Failed to initialize manager"*|*"Failed to lookup RuntimeDirectory"*)
      out=$(systemd-analyze verify "$dir/on-air-core.service" 2>&1 || true) ;;
  esac
  if echo "$out" | grep -qi "on-air-core.service:.*\(error\|fail\|unknown\|invalid\)"; then
    echo "$out" >&2
    fail "unit did not verify"
  fi
fi
grep -q '^ExecStart=/usr/bin/on-air-core$' "$dir/on-air-core.service" || fail "unit ExecStart"
grep -q '^WantedBy=default.target$' "$dir/on-air-core.service" || fail "unit WantedBy"
echo "ok $dir/on-air-core.service"

# desktop entry
if have desktop-file-validate; then
  desktop-file-validate "$dir/on-air.desktop" || fail "desktop entry did not validate"
fi
grep -q '^Exec=xdg-terminal-exec --app-id=org.omarchy.on-air -e on-air$' "$dir/on-air.desktop" || fail "desktop Exec"
grep -q '^Icon=on-air$' "$dir/on-air.desktop" || fail "desktop Icon"
echo "ok $dir/on-air.desktop"

# icon
python3 -c 'import sys, xml.dom.minidom; xml.dom.minidom.parse(sys.argv[1])' "$dir/on-air.svg" || fail "svg does not parse"
grep -q 'viewBox="0 0 64 64"' "$dir/on-air.svg" || fail "svg viewBox"
echo "ok $dir/on-air.svg"

# snippets
if have jq; then
  sed 's#//.*##' "$snippets/omarchy-menu.jsonc" | sed '$ s/,[[:space:]]*$//' | { printf '{'; cat; printf '}'; } | jq . >/dev/null || fail "omarchy-menu.jsonc is not valid JSON once wrapped"
  jq . "$snippets/shell-bar.json" >/dev/null || fail "shell-bar.json"
  sed 's#//.*##' "$snippets/waybar.jsonc" | sed '$ s/,[[:space:]]*$//' | { printf '{'; cat; printf '}'; } | jq . >/dev/null || fail "waybar.jsonc is not valid JSON once wrapped"
fi
grep -q '"exec": *"on-air bar"' "$snippets/shell-bar.json" || fail "shell-bar exec"
grep -q '"exec": *"on-air bar"' "$snippets/waybar.jsonc" || fail "waybar exec"
grep -q 'omarchy-launch-or-focus-tui on-air' "$snippets/omarchy-menu.jsonc" || fail "menu open row"
grep -q 'on-air service toggle' "$snippets/omarchy-menu.jsonc" || fail "menu service row"
echo "ok $snippets/*.json*"

if have luac; then
  # One file per call: luac 5.4.4 (Ubuntu 22.04) double-frees when it is
  # given several files at once.
  for f in "$snippets/bindings.lua" "$snippets/hyprland-rules.lua"; do
    luac -p "$f" || fail "$f does not parse"
  done
fi
grep -q 'tui = "on-air"' "$snippets/bindings.lua" || fail "bindings.lua"
grep -q 'omarchy-launch-or-focus-tui on-air' "$snippets/bindings.conf" || fail "bindings.conf"
grep -q 'org.omarchy.on-air' "$snippets/hyprland-rules.lua" || fail "hyprland-rules.lua"
grep -qF 'org\.omarchy\.on-air' "$snippets/hyprland-rules.conf" || fail "hyprland-rules.conf"
echo "ok $snippets/*.lua $snippets/*.conf"

# scripts and recipe
sh -n "$dir/install.sh" || fail "install.sh syntax"
bash -n packaging/aur/on-air/PKGBUILD || fail "PKGBUILD syntax"
grep -q '^pkgname=on-air$' packaging/aur/on-air/PKGBUILD || fail "pkgname"
grep -q 'xdg-terminal-exec' packaging/aur/on-air/PKGBUILD || fail "PKGBUILD depends on xdg-terminal-exec"
grep -q '^pkgbase = on-air$' packaging/aur/on-air/.SRCINFO || fail ".SRCINFO pkgbase"
echo "ok $dir/install.sh packaging/aur/on-air"
# install.sh must build this checkout, not the last release tag.
stage=$(mktemp -d)
trap '[ -n "$stage" ] && rm -rf -- "$stage"' EXIT
sh "$dir/install.sh" --checkout --prepare-only "$stage" >/dev/null || fail "install.sh --checkout --prepare-only failed"
[ -f "$stage/PKGBUILD" ] || fail "install.sh staged no PKGBUILD"
grep -q '^source=("on-air-${pkgver}.tar.gz")$' "$stage/PKGBUILD" || fail "staged PKGBUILD does not use the local tarball"
bash -n "$stage/PKGBUILD" || fail "staged PKGBUILD syntax"
archive=$(ls "$stage"/on-air-*.tar.gz)
for f in apps/tui/Cargo.toml packaging/omarchy/on-air.desktop packaging/omarchy/on-air-core.service Cargo.lock; do
  tar -tzf "$archive" | grep -q "^on-air-[^/]*/$f$" || fail "staged source lacks $f"
done
echo "ok $dir/install.sh builds the checkout"
# The default install: the release's on-air-bin recipe with the binary
# checksums from its SHA256SUMS, staged from a fake release over file://.
fake=$(mktemp -d)
trap '[ -n "$stage" ] && rm -rf -- "$stage"; rm -rf -- "$fake"' EXIT
mkdir -p "$fake/raw/v9.0.0/packaging/aur" "$fake/releases/download/v9.0.0"
cp -r packaging/aur/on-air-bin "$fake/raw/v9.0.0/packaging/aur/"
for f in on-air-core-serve-linux-x86_64 on-air-tui-linux-x86_64 on-air-core-serve-linux-aarch64 on-air-tui-linux-aarch64; do
  echo "$(printf %s "$f" | sha256sum | cut -d' ' -f1)  $f"
done > "$fake/releases/download/v9.0.0/SHA256SUMS"
ON_AIR_RAW_URL="file://$fake/raw" ON_AIR_RELEASE_URL="file://$fake/releases" \
  sh "$dir/install.sh" --version 9.0.0 --prepare-only "$fake/bin" >/dev/null || fail "install.sh prebuilt staging failed"
core_sum=$(printf %s on-air-core-serve-linux-x86_64 | sha256sum | cut -d' ' -f1)
grep -q "^sha256sums_x86_64=('$core_sum' '" "$fake/bin/PKGBUILD" || fail "install.sh did not pin the release checksums"
grep -q "SKIP" "$fake/bin/PKGBUILD" && fail "install.sh left a SKIP checksum"
bash -n "$fake/bin/PKGBUILD" || fail "staged on-air-bin PKGBUILD syntax"
[ -f "$fake/bin/on-air.install" ] || fail "install.sh did not stage on-air-bin's on-air.install"
sed -i '/on-air-tui-linux-aarch64/d' "$fake/releases/download/v9.0.0/SHA256SUMS"
ON_AIR_RAW_URL="file://$fake/raw" ON_AIR_RELEASE_URL="file://$fake/releases" \
  sh "$dir/install.sh" --version 9.0.0 --prepare-only "$fake/bad" >/dev/null 2>&1 &&
  fail "install.sh accepted a release missing a binary checksum"
echo "ok $dir/install.sh installs a release prebuilt"

# Release recipes: one MIT license, one version, and the prebuilt package
# installs the same files as the source build.
grep -q '^MIT License' LICENSE 2>/dev/null || fail "LICENSE is not the MIT license"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' packages/core/Cargo.toml | head -1)
for recipe in on-air on-air-bin on-air-core on-air-desktop-bin; do
  pkgbuild=packaging/aur/$recipe/PKGBUILD
  need "$pkgbuild"
  need "packaging/aur/$recipe/.SRCINFO"
  bash -n "$pkgbuild" || fail "$pkgbuild syntax"
  grep -q "^license=('MIT')$" "$pkgbuild" || fail "$pkgbuild license is not MIT"
  grep -q "^pkgver=$version$" "$pkgbuild" || fail "$pkgbuild pkgver is not $version"
  grep -q "pkgver = $version$" "packaging/aur/$recipe/.SRCINFO" || fail "$recipe .SRCINFO pkgver is not $version"
  grep -q 'licenses/' "$pkgbuild" || fail "$pkgbuild does not install the license"
done
# Each directory is a whole AUR repository: a 0BSD license for the recipe,
# a .gitignore that keeps every recipe file, and no source named like one
# of those files (makepkg would use the local file instead of downloading).
python3 - <<'PYCHECK' || fail "AUR recipe directories do not follow the submission guidelines"
import pathlib, sys
errors = []
for recipe in ("on-air", "on-air-bin", "on-air-core", "on-air-desktop-bin"):
    d = pathlib.Path("packaging/aur") / recipe
    lic = d / "LICENSE"
    if not lic.is_file() or "Permission to use, copy, modify, and/or distribute" not in lic.read_text():
        errors.append(f"{recipe}: LICENSE is not the 0BSD recipe license")
    ignore = d / ".gitignore"
    kept = set()
    if ignore.is_file():
        kept = {l[1:] for l in ignore.read_text().splitlines() if l.startswith("!")}
    else:
        errors.append(f"{recipe}: no .gitignore")
    files = {p.name for p in d.iterdir() if p.is_file()}
    for name in sorted(files - kept):
        errors.append(f"{recipe}: .gitignore does not keep {name}")
    for line in (d / ".SRCINFO").read_text().splitlines():
        key, _, value = line.strip().partition(" = ")
        if key == "source" or key.startswith("source_"):
            name = value.split("::", 1)[0] if "::" in value else value.rsplit("/", 1)[-1]
            if name in files:
                errors.append(f"{recipe}: source {name} is shadowed by the local file")
for e in errors:
    print(e, file=sys.stderr)
sys.exit(1 if errors else 0)
PYCHECK
for recipe in on-air on-air-bin; do
  grep -q '^install=on-air.install$' "packaging/aur/$recipe/PKGBUILD" || fail "$recipe has no post-install message"
  cmp -s packaging/omarchy/on-air.install "packaging/aur/$recipe/on-air.install" \
    || fail "packaging/aur/$recipe/on-air.install differs from packaging/omarchy/on-air.install"
done
sh -n packaging/omarchy/on-air.install || fail "on-air.install syntax"
grep -q 'systemctl --user enable --now on-air-core' packaging/omarchy/on-air.install || fail "on-air.install does not say how to start the service"
grep -q 'on-air-tui-linux-' packaging/aur/on-air-bin/PKGBUILD || fail "on-air-bin does not fetch the prebuilt terminal UI"
grep -q 'on-air-core-serve-linux-' packaging/aur/on-air-bin/PKGBUILD || fail "on-air-bin does not fetch the prebuilt core"
[ -f "$stage/on-air.install" ] || fail "install.sh did not stage on-air.install"
for recipe in on-air on-air-bin on-air-core on-air-desktop-bin; do
  bash packaging/aur/srcinfo.sh "packaging/aur/$recipe" | cmp -s - "packaging/aur/$recipe/.SRCINFO" \
    || fail "packaging/aur/$recipe/.SRCINFO is stale; run packaging/aur/srcinfo.sh packaging/aur/$recipe"
done
# on-air-bin downloads the packaging files from the tag; their pinned
# checksums must match this checkout or the AUR build fails.
python3 - <<'PYCHECK' || fail "on-air-bin checksums do not match the packaging files"
import hashlib, pathlib, re, sys
text = pathlib.Path("packaging/aur/on-air-bin/PKGBUILD").read_text()
paths = re.findall(r'"[^"]+::\$\{_raw\}/([^"]+)"', text)
sums = re.findall(r"'([0-9a-f]{64})'", text.split("sha256sums=(", 1)[1].split(")", 1)[0])
bad = [p for p, s in zip(paths, sums) if hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() != s]
if not paths or len(paths) != len(sums) or bad:
    print("mismatch:", bad or f"{len(paths)} sources, {len(sums)} sums", file=sys.stderr)
    sys.exit(1)
PYCHECK
echo "ok packaging/aur recipes (MIT, $version)"

# The documented polkit fallback must grant everything the inhibitor asks
# for: --what=sleep:idle needs both actions.
grep -q 'inhibit-block-sleep' docs/omarchy.md || fail "docs polkit rule lacks inhibit-block-sleep"
grep -q 'inhibit-block-idle' docs/omarchy.md || fail "docs polkit rule lacks inhibit-block-idle"
echo "ok docs/omarchy.md polkit rule"
echo "all ok"
