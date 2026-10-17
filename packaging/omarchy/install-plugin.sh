#!/bin/sh
# Install the optional Quickshell frontend. The core is installed separately.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
source_dir=$root/apps/quickshell
[ -d "$source_dir" ] || source_dir=/usr/share/on-air/quickshell
destination=${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins/on-air.patchbay
enable=
replace=
for argument in "$@"; do
  case "$argument" in
    --enable) enable=1 ;;
    --replace) replace=1 ;;
    *) echo "usage: $0 [--enable] [--replace]" >&2; exit 2 ;;
  esac
done
[ -f "$source_dir/manifest.json" ] || { echo "Cannot find the on-air Quickshell files." >&2; exit 1; }
if [ -e "$destination" ] && [ -z "$replace" ]; then
  echo "$destination already exists; use --replace to back it up and install this version." >&2
  exit 1
fi
mkdir -p "$(dirname "$destination")"
stage=$(mktemp -d "$(dirname "$destination")/.on-air.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
for file in manifest.json Widget.qml CoreClient.qml PatchbayView.qml NodeColumn.qml CableLayer.qml Rope.js; do
  cp "$source_dir/$file" "$stage/$file"
done
if command -v omarchy >/dev/null 2>&1; then
  omarchy plugin validate "$stage"
fi
if [ -e "$destination" ]; then
  backup_root=${XDG_STATE_HOME:-$HOME/.local/state}/on-air/plugin-backups
  mkdir -p "$backup_root"
  backup=$backup_root/on-air.patchbay-$(date +%Y%m%d-%H%M%S)-$$
  mv "$destination" "$backup"
  echo "Previous plugin saved to $backup"
fi
mv "$stage" "$destination"
echo "Installed $destination"
if [ -n "$enable" ]; then
  # A running shell only learns about new plugin folders on a rescan.
  omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  omarchy plugin enable on-air.patchbay
  omarchy bar move on-air.patchbay --after omarchy.clock \
    || echo 'Could not move the widget beside the clock; it is in the center section.' >&2
else
  echo 'Enable beside the clock: omarchy plugin enable on-air.patchbay'
fi
