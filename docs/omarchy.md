# on-air on Omarchy

On Omarchy (and any Arch with Hyprland) on-air is two things: the
`on-air-core` service, which streams audio and serves the phone remote, and
the `on-air` terminal UI, which controls it. No browser engine, no GPU
toolkit: the UI runs in your terminal, so it is light on old hardware and
follows every Omarchy theme the moment the terminal does. Closing the UI
never stops streaming; the service owns playback, discovery and the sleep
inhibitor.

This guide covers Omarchy 4 (the Quickshell "omarchy-shell", Lua config)
and Omarchy 3 (Waybar, Walker, `.conf` config). Every snippet below also
ships in the package under `/usr/share/doc/on-air/omarchy/`.

## Install

From a clone of this repository (needs Rust and `base-devel`):

```bash
git clone https://github.com/tsonubin/on-air.git && cd on-air
packaging/omarchy/install.sh
on-air status
```

That builds the `on-air` package from your checkout, installs it with
`makepkg -si`, enables the service, and prints the snippets below with the
path each belongs in. It edits nothing under `~/.config`. It builds the
working tree, uncommitted edits included, and keeps compiled output in
`target/omarchy/`, so later runs only recompile what changed. Run
`packaging/omarchy/install.sh --clean` to rebuild everything.

> **TODO:** AUR packages (`yay -S on-air-bin` prebuilt, `yay -S on-air` from
> source) are on hold and not published yet.

The package installs `on-air-core` and `on-air` to `/usr/bin`, the unit to
`/usr/lib/systemd/user/on-air-core.service`, a launcher entry, an icon, and
the snippets. It depends on `libpulse` (`pactl`, `parec`, `pacat` for the
PipeWire capture), `bluez-utils` (`bluetoothctl`) and `xdg-terminal-exec`
(what opens the launcher entry in your terminal). AirPlay without OwnTone
needs `python-pyatv`.

## Launch

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
