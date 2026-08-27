#!/usr/bin/env bash
# Install the on-air desktop .deb from a GitHub Release.
set -euo pipefail

REPO="${REPO:-tsonubin/on-air}"
VERSION="${VERSION:-0.1.0}"
ARCH="${ARCH:-$(dpkg --print-architecture 2>/dev/null || echo amd64)}"
FILE="on-air-desktop_${VERSION}_${ARCH}.deb"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${FILE}"

if ! command -v curl >/dev/null || ! command -v apt-get >/dev/null; then
  echo "Need curl and apt-get on PATH." >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading ${URL}"
if ! curl -fL "$URL" -o "${tmp}/${FILE}"; then
  echo "Download failed. Is v${VERSION} published and public? See INSTALL.md." >&2
  exit 1
fi

echo "Installing ${FILE}"
sudo apt-get update
sudo apt-get install -y "${tmp}/${FILE}"
