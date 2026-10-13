#!/bin/sh
# Build and install on-air on Omarchy or Arch from this clone, enable the
# core service, then print the snippets and where each one goes:
#   packaging/omarchy/install.sh
# It builds the checkout you run it from (its last commit), not a release
# tag. Nothing under ~/.config is edited; the snippets are yours to paste.
#
#   --prepare-only DIR   stage the package build in DIR and stop (used by
#                        packaging/omarchy/test.sh; needs no makepkg)
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
recipe=$root/packaging/aur/on-air
doc=/usr/share/doc/on-air/omarchy

prepare_only=
case "${1:-}" in
  --prepare-only)
    prepare_only=${2:?--prepare-only needs a directory}
    ;;
  "") ;;
  *)
    echo "usage: $0 [--prepare-only DIR]" >&2
    exit 2
    ;;
esac

# Stage: the recipe, with its source switched from the release tag to a
# tarball of this checkout's HEAD.
if [ -n "$prepare_only" ]; then
  build=$prepare_only
  mkdir -p "$build"
else
  build=$(mktemp -d "${TMPDIR:-/tmp}/on-air-build.XXXXXX")
fi
pkgver=$(sed -n 's/^pkgver=//p' "$recipe/PKGBUILD")
if [ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ]; then
  echo "note: building the last commit; uncommitted changes are not included" >&2
fi
git -C "$root" archive --format=tar.gz --prefix="on-air-$pkgver/" \
  -o "$build/on-air-$pkgver.tar.gz" HEAD
sed 's|^source=.*|source=("on-air-${pkgver}.tar.gz")|' "$recipe/PKGBUILD" > "$build/PKGBUILD"
cp "$recipe/on-air.install" "$build/on-air.install"
if [ -n "$prepare_only" ]; then
  echo "$build"
  exit 0
fi

command -v makepkg >/dev/null 2>&1 || {
  echo "makepkg not found: this installer is for Omarchy and Arch Linux." >&2
  exit 1
}

echo "== Building and installing the on-air package from $root (makepkg -si)"
(cd "$build" && makepkg -si)

echo "== Enabling the on-air-core user service"
systemctl --user daemon-reload
systemctl --user enable --now on-air-core
systemctl --user --no-pager status on-air-core | head -3 || true

echo
echo "== Omarchy integration snippets (paste the ones you want)"
show() {
  echo
  echo "---- $1  ->  $2"
  cat "$doc/$1"
}
show omarchy-menu.jsonc  '~/.config/omarchy/extensions/omarchy-menu.jsonc (Omarchy 4)'
show bindings.lua        '~/.config/hypr/bindings.lua (Omarchy 4)'
show bindings.conf       '~/.config/hypr/bindings.conf (Omarchy 3)'
show hyprland-rules.lua  '~/.config/hypr/hyprland.lua (Omarchy 4, optional floating window)'
show hyprland-rules.conf '~/.config/hypr/hyprland.conf (Omarchy 3, optional floating window)'
show shell-bar.json      '~/.config/omarchy/shell.json under bar.layout.right (Omarchy 4)'
show waybar.jsonc        '~/.config/waybar/config.jsonc (Omarchy 3)'
echo
echo "Launch on-air from the app launcher (Super+Space), or run: on-air"
echo "The build directory $build can be deleted."
