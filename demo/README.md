# Demo recording

`ON_AIR_DEMO=1` turns the hardware-free mock core into a believable setup
for a screen recording:

- one input, **USB Audio Capture**
- two speakers: **Bedroom HomePods** (AirPlay stereo pair) and **Living Room
  Sonos**
- speakers take about a second to answer, so the "starting" step is visible
- while an output is live, the level meter moves with a synthetic beat
  instead of sitting at zero
- the real sleep inhibitor is held, so `on-air status` matches production

Demo mode is mock mode. It never reads or writes `~/.config/on-air-core`, so
your real settings, pairings and patchbay routes are left alone. Every
restart begins from a clean, idle state.

## Swap in, record, restore

```bash
demo/swap-in.sh     # build, then run the demo core as the on-air-core service
# record: `on-air` in a terminal, the bar widget beside the clock, the phone
systemctl --user restart on-air-core   # reset to idle between takes
demo/restore.sh     # back to /usr/bin/on-air-core and your settings
```

`swap-in.sh` copies the release binary to `~/.local/share/on-air-demo/` and
adds `~/.config/systemd/user/on-air-core.service.d/zz-demo.conf`.
`restore.sh` removes both. The installed `on-air` CLI and the bar widget are
not changed. They are pointed at :47990 as usual and find the demo core
there.

## During the take

- The pairing PIN is always `123456`.
- Don't press `a` in the TUI. It enables or disables login autostart of the
  real unit.
- Run the phone remote on another device on the same network. Its scan
  finds the demo core on :47990, or choose *Set up manually* and type this
  computer's LAN address. If UFW is active, allow the port first:
  `sudo ufw allow 47990/tcp`.
