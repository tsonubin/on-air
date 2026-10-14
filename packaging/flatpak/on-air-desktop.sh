#!/bin/sh
# Flatpak entry point. Capture, Bluetooth, and AirPlay shell out to host
# tools (pactl, parec, pacat, bluetoothctl, python3, systemd-inhibit). Those
# binaries talk to the host's PipeWire and BlueZ, so they run through
# flatpak-spawn --host. The GUI itself stays in the sandbox.
set -eu

tools="pactl parec pacat bluetoothctl python3 systemd-inhibit uv"
shim_dir="${XDG_RUNTIME_DIR:-/tmp}/on-air-host-tools"

if command -v flatpak-spawn >/dev/null 2>&1; then
  mkdir -p "$shim_dir"
  for tool in $tools; do
    printf '#!/bin/sh\nexec flatpak-spawn --host %s "$@"\n' "$tool" > "$shim_dir/$tool"
    chmod 755 "$shim_dir/$tool"
  done
  export PATH="$shim_dir:${PATH}"
fi

exec /app/bin/on-air-desktop-bin "$@"
