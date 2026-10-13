#!/bin/sh
# Build and install on-air on Omarchy or Arch, enable the core service,
# then print the snippets and where each one goes. Run from a clone:
#   packaging/omarchy/install.sh
# Nothing under ~/.config is edited; the snippets are yours to paste.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
recipe=$root/packaging/aur/on-air
doc=/usr/share/doc/on-air/omarchy

command -v makepkg >/dev/null 2>&1 || {
  echo "makepkg not found: this installer is for Omarchy and Arch Linux." >&2
  exit 1
}

echo "== Building and installing the on-air package (makepkg -si)"
(cd "$recipe" && makepkg -si)

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
