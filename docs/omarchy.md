# on-air on Omarchy

The `on-air-core` service owns audio, discovery and the sleep inhibitor.
Omarchy 4 can control it through a small native Quickshell panel beside the
clock, with system defaults, input mixes and local/network output groups.
The `on-air` terminal UI remains available on Omarchy and other Linux
desktops. Closing either interface never stops playback.

This guide covers Omarchy 4 (the Quickshell "omarchy-shell", Lua config)
and Omarchy 3 (Waybar, Walker, `.conf` config). Every snippet below also
ships in the package under `/usr/share/doc/on-air/omarchy/`.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/tsonubin/on-air/main/packaging/omarchy/install.sh | sh
on-air status
```

The installer takes the latest release's prebuilt binaries, checks them
against the release's `SHA256SUMS`, and installs them as the `on-air-bin`
pacman package with `sudo pacman -U`. Nothing compiles, which matters on
older laptops. It then enables the `on-air-core` user service, restarts it
so an update replaces the running core, and prints the snippets below with
the path each belongs in. It edits nothing under `~/.config`. Run it
again to update. `pacman -R on-air-bin` removes it. Prebuilt installs
need a release with the terminal UI binaries and the recipes (v0.2.0 or
newer; v0.1.0 predates them).

From a clone of this repository (needs Rust and `base-devel`), the same
script takes `--checkout` to build the `on-air` package from your working
tree (uncommitted edits included; compiled output stays in
`target/omarchy/`, so later runs only recompile what changed —
`--clean` rebuilds everything), `--source` to build the release from
source instead, and `--version X.Y.Z` to pick a release. Installation
skips the Rust test suite. For development, run
`cargo test --locked -p on-air-core -p on-air-tui` separately.

Run the installer as your normal user in a terminal. The install step uses
your normal sudo authentication and preserves cached credentials. If
installation fails, it prints the saved package path so you can retry with
`sudo pacman -U -- "/path/to/package.pkg.tar.zst"` without rebuilding.

> **TODO:** AUR packages (`yay -S on-air-bin` prebuilt, `yay -S on-air` from
> source) are on hold and not published yet.

The package installs `on-air-core` and `on-air` to `/usr/bin`, the unit to
`/usr/lib/systemd/user/on-air-core.service`, a launcher entry, an icon, and
the snippets. It depends on `libpulse` (`pactl`, `parec`, `pacat` for the
PipeWire capture), `bluez-utils` (`bluetoothctl`) and `xdg-terminal-exec`
(what opens the launcher entry in your terminal). AirPlay goes through
OwnTone (`owntone-server` in the AUR) when it runs, otherwise through
pyatv: install `uv` and fetch pyatv once with
`uv run --with pyatv python3 -c "import pyatv"`, since the core runs it
offline.

## Launch

### Native panel (Omarchy 4)

After installing the core, install and enable the optional widget:

```sh
sh packaging/omarchy/install-plugin.sh --enable
# From the installed Arch package, the same command is:
on-air-install-plugin --enable
```

Use one of these commands. The widget is placed beside the centered clock;
it can be moved with Omarchy's normal bar gestures. Updates use `--replace`
and preserve the previous copy outside the plugin search path.

Click a row for a system default, drag row bodies together to make a combo,
or drag ports across the gap to connect audio. Groups start collapsed and
the one footer slider follows the selection. Right-click for connection
targets, mute and disconnect; Escape closes. The panel inherits the active
Omarchy palette, font, border, scaling and reduced-motion preference.

The **Desktop** source receives apps sent to **On-Air desktop**. Discovered
AirPlay/Sonos speakers are also selectable directly as system outputs.
Independent AirPlay needs pyatv (through `uv`, as in Install) and a
receiver address from LAN discovery. Output groups use independent
timing, and the network path streams stereo PCM. Physical speaker
buffering remains.

See [the panel guide](../apps/quickshell/README.md) for all gestures, keyboard
controls, state persistence and the Qt preview. The sections below describe
the terminal interface and the existing command-style bar integration.

### Terminal

- **App launcher** (`Super+Space`): the entry is called on-air. It opens
  your default terminal with the Wayland app id `org.omarchy.on-air`.
- **Terminal**: `on-air`. `on-air --help` lists the subcommands.
- **Hotkey**: `Super+Shift+Alt+O`, once you add the binding below. It
  raises the running window instead of opening a second one.

The window tiles like any terminal. To open it floating and centered
(875×600, the same tag Omarchy gives btop), add the rule:

Omarchy 4, `~/.config/hypr/hyprland.lua`:

```lua
o.window("org.omarchy.on-air", { tag = "+floating-window" })
```

Omarchy 3, `~/.config/hypr/hyprland.conf`:

```
windowrule = tag +floating-window, match:class ^(org\.omarchy\.on-air)$
```

## Keybinding

Omarchy 4, `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + SHIFT + ALT + O", "on-air", { tui = "on-air", focus = true })
```

Omarchy 3, `~/.config/hypr/bindings.conf`:

```
bindd = SUPER SHIFT ALT, O, on-air, exec, omarchy-launch-or-focus-tui on-air
```

## Menu (Omarchy 4)

Add rows to `~/.config/omarchy/extensions/omarchy-menu.jsonc`, inside the
top-level object. The dotted ids place them: `on-air` on the root menu,
the rest inside it.

```jsonc
"on-air": {"icon":"󰝚","label":"on-air"},
"on-air.open": {"icon":"󰝚","label":"Open","action":"omarchy-launch-or-focus-tui on-air"},
"on-air.monitor": {"icon":"󰐥","label":"Monitor on/off","action":"on-air monitor toggle"},
"on-air.restart": {"icon":"󰜉","label":"Restart core","action":"on-air service restart"},
```

## Bar

`on-air bar` prints one Waybar-style JSON line: a speaker glyph, the live
speaker's name when one is playing, and a `class` of `live`, `idle`, `off`
or `down` for styling.

Omarchy 4, in `~/.config/omarchy/shell.json` under `bar.layout.right` (or
whichever section you like):

```json
{ "id": "on-air", "type": "command", "exec": "on-air bar", "interval": 5,
  "tooltip": "on-air", "onClick": "omarchy-launch-or-focus-tui on-air" }
```

Omarchy 3, in `~/.config/waybar/config.jsonc`, then list `custom/on-air`
in `modules-right`:

```jsonc
"custom/on-air": {
  "exec": "on-air bar",
  "return-type": "json",
  "interval": 5,
  "on-click": "omarchy-launch-or-focus-tui on-air"
},
```

## Idle and sleep

While the service is on, the core holds a logind sleep inhibitor
(`systemd-inhibit --what=sleep:idle --mode=block`), so the laptop does not
suspend mid-stream. The display may still turn off and Omarchy's idle lock
still fires; that is intended. Check with:

```bash
on-air status | grep "sleep inhibited"
```

If it says `no` while the service is on, logind refused the inhibitor to
the user service. The TUI header shows `sleep not inhibited` in that case.
Two fixes:

- Omarchy's own toggle, per session: `omarchy-toggle-idle stay-awake`
  (`Super+Ctrl+I` on Omarchy 4), `allow-idle` to undo.
- A polkit rule, permanent. As root, create
  `/etc/polkit-1/rules.d/50-on-air-inhibit.rules`:

  ```js
  polkit.addRule(function (action, subject) {
    if ((action.id == "org.freedesktop.login1.inhibit-block-sleep" ||
         action.id == "org.freedesktop.login1.inhibit-block-idle") &&
        subject.user == "YOUR_USERNAME") {
      return polkit.Result.YES;
    }
  });
  ```

  The core asks for both sleep and idle locks in one request, so the rule
  has to grant both.

  then `systemctl --user restart on-air-core`.

## Using the TUI

Run `on-air` in a terminal. Quitting never stops the service.

Click `[r Refresh]` in the Destination pane or press `r` to reload devices and status.

### Keys

| Key | Action |
|---|---|
| `Tab / Shift-Tab` | Next / previous pane |
| `j` / `k` | Move within the pane |
| `Enter` | Activate the source or output (opens pairing when needed) |
| `h / l, ← / →` | Mixer: adjust the focused row |
| `H / L` | Fine adjust (volume ±1, EQ ±0.5 dB) |
| `↑ / ↓` (`+ / -` also work) | Volume ±5 from any pane |
| `0` | Mixer: reset the focused EQ band |
| `x` | Stop the live output |
| `Space` | CD play / pause |
| `> / <` | CD next / previous track |
| `[ / ]` | CD seek back / forward 10 s |
| `g` | CD go to track |
| `e` | CD eject (asks first) |
| `m` | Monitor on / off |
| `a` | Start the service at login on / off |
| `i` | Status: version, addresses, monitor, login start |
| `b` | Open Bluetooth settings |
| `r` | Refresh devices |
| `f` | Firewall help when UFW is blocking LAN audio |
| `t` | Audio check and capture buffer settings |
| `?` | This help |
| `Esc` | Close, dismiss the error or notice |
| `q / Ctrl-C` | Quit; the service keeps running |

### Audio performance and buffering

Open **Audio performance** in the desktop GUI, **Sound → Audio format** in
the remote app, or press `t` in the TUI. The CLI offers the same controls:

```bash
on-air audio check
on-air audio check --json
on-air audio buffer 100
on-air audio buffer default
```

Run the short check during your usual workload. It sends synthetic samples
through an isolated copy of the real EQ, metering and rate-conversion path,
and measures pipeline round trips and scheduler delays. Playback continues.
The recommendation includes three times the observed worst timing plus one
audio quantum, rounded up to 50, 100, 200 or 400 ms. This provides timing
headroom for other work without changing process priority or reserving CPU.
If the measured stalls exceed the available buffer budget, the result says
so rather than claiming that a larger buffer will fix them.

Checking never applies a setting. Use **Apply recommendation**, select a
capture buffer manually, or run `on-air audio buffer N`. The setting is
saved on the host and shared across clients. In the TUI, left/right selects
a buffer and `a` applies it. Larger buffers trade more capture delay for
tolerance of scheduling stalls; actual sizes depend on the audio driver.
The check does not measure USB capture errors or network/speaker buffering.

### UFW blocking LAN audio

On Linux hosts with readable kernel journal logs, the core looks for recent
UFW TCP drops on its listening port. The TUI, GUI and remote app show the
same warning. Healthy radio playback clears a stale audio warning.

Click the warning or press `f` in the TUI, or open **Firewall help** in the
GUI. **Allow LAN audio** (Enter in the TUI) asks for administrator
authentication on the host and adds a rule for the observed local subnet
and on-air's TCP port. It also handles UFW's inconsistent state where its
config says disabled but old kernel rules are still blocking traffic.
Other firewall rules stay in place.

```bash
on-air firewall          # inspect the warning and manual commands
on-air firewall allow    # explicitly request administrator-authorized repair
```

Remote apps show selectable commands and direct you to authorize on the
host. Remote API clients cannot launch host administrator prompts.

### Mouse

Mouse capture is on; it is turned off again on exit. Hold Shift while
dragging to select text. If capture cannot be enabled the keyboard still
works and the help overlay says "mouse off".

| Where | Action |
|---|---|
| Source or output row | Left click: focus the pane, move the cursor and select (same as `Enter`). Wheel: move the cursor |
| Volume or EQ fader | Left click: focus the fader; click it again, or drag, to set the value at that cell (the top is the maximum). A drag stays on the fader where the button went down. Wheel: fine step. In the compact mixer a click only focuses the fader and a drag does nothing there |
| Rate row | Left click: next rate. Wheel: next or previous |
| CD buttons | Left click: previous, play / pause, next, eject (eject still asks first) |
| CD progress bar | Left click: seek to that position |
| `monitor off` tag | Left click: turn the monitor on or off |
| `PIN` or address tag | Left click: open the status overlay |
| Outside an open overlay | Left click: close it (same as `Esc`) |

### Status

`i` opens the status overlay: version, the core's port, addresses (each
with the port), monitor state and whether the service starts at login.
Click the `PIN` or address tag in the header for the same view. `Esc` or
`i` closes it; `q` quits.

### Monitor off

With the monitor off the header shows `monitor off — press m`. Capture and
streaming are stopped, so the mixer, sources, outputs and CD controls are
locked until you press `m` (or click the tag) to turn it back on. `i`, `?`,
`a`, `Esc`, `q` and `Ctrl-C` still work.

## Bluetooth

Press `b` in the TUI to open Bluetooth settings. On Omarchy 4 that toggles
the shell's Bluetooth panel (`Super+Ctrl+B`); on Omarchy 3 it opens
`bluetui`. Pair the speaker there, then pick it in the Outputs pane.

## Old Macs

The 2013 and 2014 15-inch MacBook Pro pair a Haswell Intel GPU with a
Kepler NVIDIA chip that modern NVIDIA drivers no longer support. The
working setup reported in the Omarchy community is `nouveau` with Hyprland
rendering on the Intel GPU. Create `~/.config/uwsm/env.d/20-intel-renderer`:

```sh
export AQ_NO_KMS_REQUIREMENT=1
export AQ_DRM_DEVICES="/dev/dri/card1:/dev/dri/card0"
```

Check which card is Intel with `cat /sys/class/drm/card*/device/vendor`
(`0x8086` is Intel) and put that card first. Then log out and in.

With or without a discrete GPU, `omarchy toggle animations off` turns off
the animations, blur and shadows that are slow without GPU acceleration.
The TUI itself needs none of that.

## Uninstall

```bash
systemctl --user disable --now on-air-core
sudo pacman -Rns on-air
```

Then remove the snippets you pasted into your Omarchy config. The core keeps
its settings and paired phones in `~/.config/on-air-core/`; delete that
directory for a clean slate.
