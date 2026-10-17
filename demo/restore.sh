#!/usr/bin/env bash
# Undo demo/swap-in.sh: drop the demo override and restart the real
# /usr/bin/on-air-core with the user's own settings.
set -euo pipefail

rm -f "$HOME/.config/systemd/user/on-air-core.service.d/zz-demo.conf"
rm -rf "$HOME/.local/share/on-air-demo"
systemctl --user daemon-reload
systemctl --user restart on-air-core
for _ in $(seq 50); do
  curl -sf http://127.0.0.1:47990/api/status >/dev/null && break
  sleep 0.2
done
if [[ $(systemctl --user show on-air-core -p MainPID --value) == 0 ]] ||
  [[ $(readlink "/proc/$(systemctl --user show on-air-core -p MainPID --value)/exe") == *on-air-demo* ]]; then
  echo "on-air-core is not back on the production binary" >&2
  exit 1
fi
echo "production on-air-core restored."
