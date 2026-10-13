#!/bin/sh
# Install on-air on Omarchy or Arch as a pacman package, enable the core
# service, then print the Omarchy snippets and where each one goes:
#
#   curl -fsSL https://raw.githubusercontent.com/tsonubin/on-air/main/packaging/omarchy/install.sh | sh
#
# By default it installs the latest release's prebuilt binaries (the
# on-air-bin recipe from that release's tag), checked against the release's
# SHA256SUMS, so nothing compiles. Nothing under ~/.config is edited; the
# snippets are yours to paste. Run it again to update.
#
#   --version X.Y.Z      that release instead of the latest
#   --source             build the release from source (the on-air recipe;
#                        makepkg installs Rust, slow on older laptops)
#   --checkout           build the last commit of the clone this script is in
#   --prepare-only DIR   stage the package build in DIR and stop (used by
#                        packaging/omarchy/test.sh; needs no makepkg)
#
# ON_AIR_REPO (default tsonubin/on-air), ON_AIR_RAW_URL and
# ON_AIR_RELEASE_URL override where the recipe and the release come from.
set -eu

repo=${ON_AIR_REPO:-tsonubin/on-air}
raw_base=${ON_AIR_RAW_URL:-https://raw.githubusercontent.com/$repo}
release_base=${ON_AIR_RELEASE_URL:-https://github.com/$repo/releases}
doc=/usr/share/doc/on-air/omarchy

die() {
  echo "on-air install: $*" >&2
  exit 1
}

fetch() {
  curl -fsSL --retry 3 -o "$2" "$1" || die "could not download $1"
}

# The tag GitHub's /releases/latest redirects to, without the leading v.
latest_version() {
  url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$release_base/latest") ||
    die "could not reach $release_base/latest"
  case "$url" in
    */tag/v*) echo "${url##*/tag/v}" ;;
    *) die "no published release found for $repo" ;;
  esac
}

# Hash of one release asset from SHA256SUMS.
release_hash() {
  hash=$(awk -v f="$2" '$2 == f || $2 == "*" f { print $1; found = 1 } END { if (!found) exit 1 }' "$1") ||
    die "the release's SHA256SUMS has no entry for $2"
  case "$hash" in
    *[!0-9a-f]* | "") die "the release's SHA256SUMS entry for $2 is not a SHA-256" ;;
  esac
  [ ${#hash} -eq 64 ] || die "the release's SHA256SUMS entry for $2 is not a SHA-256"
  echo "$hash"
}

# The on-air-bin recipe from the release's tag, with the binary checksums
# taken from the release's SHA256SUMS (the tag's own recipe predates them).
stage_release_bin() {
  sums=$build/SHA256SUMS
  curl -fsSL --retry 3 -o "$sums" "$release_base/download/v$version/SHA256SUMS" ||
    die "release v$version is not published (or is still building): no SHA256SUMS"
  base=$raw_base/v$version/packaging/aur/on-air-bin
  fetch "$base/PKGBUILD" "$build/PKGBUILD"
  fetch "$base/on-air.install" "$build/on-air.install"
  # One lookup per assignment, so set -e stops on any missing asset.
  core_x86=$(release_hash "$sums" on-air-core-serve-linux-x86_64)
  tui_x86=$(release_hash "$sums" on-air-tui-linux-x86_64)
  core_arm=$(release_hash "$sums" on-air-core-serve-linux-aarch64)
  tui_arm=$(release_hash "$sums" on-air-tui-linux-aarch64)
  sed -e "s|^sha256sums_x86_64=.*|sha256sums_x86_64=('$core_x86' '$tui_x86')|" \
    -e "s|^sha256sums_aarch64=.*|sha256sums_aarch64=('$core_arm' '$tui_arm')|" \
    "$build/PKGBUILD" > "$build/PKGBUILD.new"
  mv "$build/PKGBUILD.new" "$build/PKGBUILD"
  rm -f "$sums"
}

# The on-air recipe from the release's tag; it builds that tag from source.
stage_release_source() {
  base=$raw_base/v$version/packaging/aur/on-air
  fetch "$base/PKGBUILD" "$build/PKGBUILD"
  fetch "$base/on-air.install" "$build/on-air.install"
}

# The on-air recipe with its source switched from the release tag to a
# tarball of this clone's HEAD.
stage_checkout() {
  root=$(cd "$(dirname "$0")/../.." 2>/dev/null && pwd) || root=
  recipe=$root/packaging/aur/on-air
  [ -n "$root" ] && [ -f "$recipe/PKGBUILD" ] ||
    die "--checkout needs the script run from a clone: packaging/omarchy/install.sh --checkout"
  pkgver=$(sed -n 's/^pkgver=//p' "$recipe/PKGBUILD")
  if [ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ]; then
    echo "note: building the last commit; uncommitted changes are not included" >&2
  fi
  git -C "$root" archive --format=tar.gz --prefix="on-air-$pkgver/" \
    -o "$build/on-air-$pkgver.tar.gz" HEAD
  sed 's|^source=.*|source=("on-air-${pkgver}.tar.gz")|' "$recipe/PKGBUILD" > "$build/PKGBUILD"
  cp "$recipe/on-air.install" "$build/on-air.install"
}

usage() {
  cat <<'USAGE'
usage: install.sh [--version X.Y.Z] [--source | --checkout]
  (default)        the latest release, prebuilt, checked against its SHA256SUMS
  --version X.Y.Z  that release instead of the latest
  --source         build the release from source (needs Rust; slow on older laptops)
  --checkout       build the last commit of the clone this script is in
USAGE
}

show() {
  echo
  echo "---- $1  ->  $2"
  cat "$doc/$1"
}

main() {
  mode=bin
  version=
  prepare_only=
  while [ $# -gt 0 ]; do
    case "$1" in
      --version) version=${2:?--version needs a version, like 0.2.0}; shift ;;
      --source) mode=source ;;
      --checkout) mode=checkout ;;
      --prepare-only) prepare_only=${2:?--prepare-only needs a directory}; shift ;;
      -h | --help) usage; exit 0 ;;
      *) die "unknown option $1 (try --help)" ;;
    esac
    shift
  done
  version=${version#v}
  case "$version" in
    *[!0-9A-Za-z.+-]*) die "not a version: $version" ;;
  esac

  if [ -z "$prepare_only" ]; then
    command -v makepkg >/dev/null 2>&1 || die "makepkg not found: this installer is for Omarchy and Arch Linux"
    [ "$(id -u)" -ne 0 ] || die "run this as your user, not root; makepkg asks for sudo when it installs"
  fi
  command -v curl >/dev/null 2>&1 || [ "$mode" = checkout ] || die "curl not found"

  if [ -n "$prepare_only" ]; then
    build=$prepare_only
    mkdir -p "$build"
  else
    build=$(mktemp -d "${TMPDIR:-/tmp}/on-air-build.XXXXXX")
  fi

  case "$mode" in
    bin | source)
      [ -n "$version" ] || version=$(latest_version)
      case "$version" in
        "" | *[!0-9A-Za-z.+-]*) die "unexpected latest release tag: v$version" ;;
      esac
      echo "== on-air $version ($([ "$mode" = bin ] && echo prebuilt || echo from source))"
      if [ "$mode" = bin ]; then stage_release_bin; else stage_release_source; fi
      ;;
    checkout)
      stage_checkout
      ;;
  esac
  if [ -n "$prepare_only" ]; then
    echo "$build"
    exit 0
  fi

  echo "== Building and installing the package (makepkg -si)"
  # Piped into sh, stdin is the script: answer pacman's prompts from the terminal.
  if [ ! -t 0 ] && [ -r /dev/tty ]; then
    (cd "$build" && makepkg -si </dev/tty)
  else
    (cd "$build" && makepkg -si)
  fi
  rm -rf "$build"

  echo "== Enabling the on-air-core user service"
  systemctl --user daemon-reload
  systemctl --user enable on-air-core
  # restart, not start: an update must replace the running core.
  systemctl --user restart on-air-core
  systemctl --user --no-pager status on-air-core | head -3 || true

  echo
  echo "== Omarchy integration snippets (paste the ones you want)"
  show omarchy-menu.jsonc  '~/.config/omarchy/extensions/omarchy-menu.jsonc (Omarchy 4)'
  show bindings.lua        '~/.config/hypr/bindings.lua (Omarchy 4)'
  show bindings.conf       '~/.config/hypr/bindings.conf (Omarchy 3)'
  show hyprland-rules.lua  '~/.config/hypr/hyprland.lua (Omarchy 4, optional floating window)'
  show hyprland-rules.conf '~/.config/hypr/hyprland.conf (Omarchy 3, optional floating window)'
  show shell-bar.json      '~/.config/omarchy/shell.json under bar.layout.right (Omarchy 4)'
  show waybar.jsonc        '~/.config/waybar/config.jsonc (Omarchy 3)'
  echo
  echo "Launch on-air from the app launcher (Super+Space), or run: on-air"
}

# Everything runs from main, so a download cut short by `curl | sh` runs nothing.
main "$@"
