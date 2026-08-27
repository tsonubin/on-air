#!/usr/bin/env bash
# Build the on-air-core .deb from this source tree (native Debian package).
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

if [ ! -e debian ]; then
  ln -s packaging/debian debian
elif [ ! -L debian ] && [ debian -ef packaging/debian ]; then
  :
elif [ ! -L debian ]; then
  echo "A non-symlink debian/ directory already exists; refusing to overwrite." >&2
  exit 1
fi

if ! command -v dpkg-buildpackage >/dev/null; then
  echo "Install dpkg-dev and debhelper (e.g. sudo apt install dpkg-dev debhelper cargo rustc)." >&2
  exit 1
fi

dpkg-buildpackage -us -uc -b
echo "Look for on-air-core_*.deb in $(dirname "$root")"
