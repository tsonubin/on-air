#!/usr/bin/env bash
# Run the demo core (ON_AIR_DEMO=1) as the on-air-core user service, so the
# installed `on-air` CLI, the Omarchy bar widget and the phone see it on
# :47990 exactly as they would see the real one. Undo with demo/restore.sh.
#
# Demo mode is mock mode: it never reads or writes ~/.config/on-air-core,
# so the real settings, pairings and patchbay routes are left as they were.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bin_dir="$HOME/.local/share/on-air-demo"
dropin_dir="$HOME/.config/systemd/user/on-air-core.service.d"

cargo build --release --locked -p on-air-core --bin on-air-core \
  --manifest-path "$root/Cargo.toml"
mkdir -p "$bin_dir" "$dropin_dir"
# A copy, so rebuilding the branch cannot change the binary mid-recording.
install -m 755 "$root/target/release/on-air-core" "$bin_dir/on-air-core"

cat >"$dropin_dir/zz-demo.conf" <<EOF
# Written by on-air demo/swap-in.sh; demo/restore.sh removes it.
[Service]
ExecStart=
ExecStart=$bin_dir/on-air-core
Environment=ON_AIR_DEMO=1
EOF

systemctl --user daemon-reload
systemctl --user restart on-air-core
for _ in $(seq 50); do
  curl -sf http://127.0.0.1:47990/api/inputs | grep -q "USB Audio Capture" && break
  sleep 0.2
done
curl -sf http://127.0.0.1:47990/api/inputs | grep -q "USB Audio Capture" || {
  echo "the demo core did not come up; run demo/restore.sh" >&2
  exit 1
}
echo "demo core is serving :47990 as on-air-core (pairing PIN 123456)."
echo "restore production with: demo/restore.sh"
