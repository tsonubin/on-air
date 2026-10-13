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
sh "$dir/install.sh" --prepare-only "$stage" >/dev/null || fail "install.sh --prepare-only failed"
[ -f "$stage/PKGBUILD" ] || fail "install.sh staged no PKGBUILD"
grep -q '^source=("on-air-${pkgver}.tar.gz")$' "$stage/PKGBUILD" || fail "staged PKGBUILD does not use the local tarball"
bash -n "$stage/PKGBUILD" || fail "staged PKGBUILD syntax"
archive=$(ls "$stage"/on-air-*.tar.gz)
for f in apps/tui/Cargo.toml packaging/omarchy/on-air.desktop packaging/omarchy/on-air-core.service Cargo.lock; do
  tar -tzf "$archive" | grep -q "^on-air-[^/]*/$f$" || fail "staged source lacks $f"
done
echo "ok $dir/install.sh builds the checkout"

# The documented polkit fallback must grant everything the inhibitor asks
# for: --what=sleep:idle needs both actions.
grep -q 'inhibit-block-sleep' docs/omarchy.md || fail "docs polkit rule lacks inhibit-block-sleep"
grep -q 'inhibit-block-idle' docs/omarchy.md || fail "docs polkit rule lacks inhibit-block-idle"
echo "ok docs/omarchy.md polkit rule"
echo "all ok"
