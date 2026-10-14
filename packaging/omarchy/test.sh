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
grep -q 'on-air monitor toggle' "$snippets/omarchy-menu.jsonc" || fail "menu monitor row"
grep -q 'on-air service restart' "$snippets/omarchy-menu.jsonc" || fail "menu restart row"
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
# install.sh must build this checkout in place, not the last release tag.
stage=$(mktemp -d)
trap '[ -n "$stage" ] && rm -rf -- "$stage"' EXIT
sh "$dir/install.sh" --prepare-only "$stage" >/dev/null || fail "install.sh --prepare-only failed"
[ -f "$stage/PKGBUILD" ] || fail "install.sh staged no PKGBUILD"
grep -q '^source=()$' "$stage/PKGBUILD" || fail "staged PKGBUILD still downloads a source"
grep -q '^export CARGO_TARGET_DIR=target/omarchy$' "$stage/PKGBUILD" || fail "staged PKGBUILD does not use target/omarchy"
grep -q 'target/omarchy/release/on-air-core' "$stage/PKGBUILD" || fail "staged package() does not read target/omarchy"
! grep -q 'CARGO_TARGET_DIR=target$' "$stage/PKGBUILD" || fail "staged build() still overrides CARGO_TARGET_DIR"
bash -n "$stage/PKGBUILD" || fail "staged PKGBUILD syntax"
linked=$(ls -d "$stage"/src/on-air-*)
[ -L "$linked" ] && [ "$(cd "$linked" && pwd -P)" = "$(pwd -P)" ] || fail "staged source is not a link to this checkout"
sh "$dir/install.sh" --bogus >/dev/null 2>&1 && fail "install.sh accepted an unknown option"
echo "ok $dir/install.sh builds the checkout"
python3 "$dir/test-install.py" || fail "install.sh installation flow"
echo "ok $dir/install.sh authentication and failure handling"

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
