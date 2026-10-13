#!/usr/bin/env bash
# Print the .SRCINFO for one AUR recipe directory, like
# `makepkg --printsrcinfo`, for machines without makepkg (CI, macOS).
# Prefer makepkg when it exists; the AUR accepts either.
#   packaging/aur/srcinfo.sh packaging/aur/on-air-bin > packaging/aur/on-air-bin/.SRCINFO
set -euo pipefail

dir=${1:?usage: srcinfo.sh <recipe-dir>}
if command -v makepkg >/dev/null 2>&1; then
  (cd "$dir" && makepkg --printsrcinfo)
  exit 0
fi

(
  cd "$dir"
  # shellcheck disable=SC1091
  source ./PKGBUILD
  field() {
    local key=$1
    shift
    local value
    for value in "$@"; do
      printf '\t%s = %s\n' "$key" "$value"
    done
  }
  printf 'pkgbase = %s\n' "$pkgname"
  field pkgdesc "$pkgdesc"
  field pkgver "$pkgver"
  field pkgrel "$pkgrel"
  field url "$url"
  [ -n "${install:-}" ] && field install "$install"
  field arch "${arch[@]}"
  field license "${license[@]}"
  [ -n "${makedepends+x}" ] && field makedepends "${makedepends[@]}"
  [ -n "${depends+x}" ] && field depends "${depends[@]}"
  [ -n "${optdepends+x}" ] && field optdepends "${optdepends[@]}"
  [ -n "${provides+x}" ] && field provides "${provides[@]}"
  [ -n "${conflicts+x}" ] && field conflicts "${conflicts[@]}"
  [ -n "${options+x}" ] && field options "${options[@]}"
  [ -n "${source+x}" ] && field source "${source[@]}"
  [ -n "${sha256sums+x}" ] && field sha256sums "${sha256sums[@]}"
  for a in "${arch[@]}"; do
    declare -n s="source_$a" sums="sha256sums_$a"
    [ -n "${s+x}" ] && field "source_$a" "${s[@]}"
    [ -n "${sums+x}" ] && field "sha256sums_$a" "${sums[@]}"
    unset -n s sums
  done
  printf '\npkgname = %s\n' "$pkgname"
)
