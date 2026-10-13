# on-air terminal UI

`on-air` is the Linux control surface: a ratatui terminal UI over the
`on-air-core` service on `http://127.0.0.1:47990`, plus three subcommands
for Omarchy's bar and menu. It owns no audio; quitting it never stops
streaming. See [`docs/omarchy.md`](../../docs/omarchy.md) for the
Omarchy integration (launcher entry, keybinding, menu rows, bar module).

```bash
# dev loop: a hardware-free core in one terminal, the UI in another
ON_AIR_MOCK=1 PORT=47990 cargo run -p on-air-core --bin on-air-core
cargo run -p on-air-tui                       # the UI
cargo run -p on-air-tui -- bar                # one Waybar-style JSON line
cargo run -p on-air-tui -- status [--json]    # the core's state
cargo run -p on-air-tui -- service on|off|toggle

cargo test -p on-air-tui                      # unit tests + the mock core in-process
cargo clippy --locked -p on-air-tui --all-targets -- -D warnings
```

`--base URL` points any form at another core.

## Keys

The layout follows cliamp, the music TUI Omarchy preinstalls: `Tab`
cycles focus, `j`/`k` move, `h`/`l` adjust, `+`/`-` are volume, `Space`
is transport, `?` is help and `q` quits.

| Key | Action |
| --- | --- |
| `Tab`, `Shift-Tab` | Next / previous pane (Sources → Outputs → Mixer) |
| `j`/`k`, `↓`/`↑` | Move within the pane |
| `Enter` | Sources: activate input. Outputs: activate (opens pairing when the device needs it) |
| `h`/`l`, `←`/`→` | Mixer: adjust the focused row (EQ ±1 dB, rates through the catalog, volume ±5) |
| `+`/`-`, `=`/`_` | Volume ±5 from any pane |
| `0` | Mixer: reset the focused EQ band to 0 dB |
| `x` | Stop the live output |
| `Space` | CD play/pause when a disc is present |
| `>`/`<`, `.`/`,` | CD next / previous track |
| `s` | Service on / off |
| `a` | Start the service at login on / off (the `on-air-core` systemd user unit) |
| `b` | Open Bluetooth settings |
| `r` | Refresh devices and snapshot |
| `?` | Help overlay |
| `Esc` | Close the overlay or modal, dismiss the error banner |
| `q`, `Ctrl-C` | Quit; the service keeps running |

## Layout of `src/`

- `main.rs` — argument parsing, terminal lifecycle, the event loop.
- `model.rs` — the whole UI state; `update.rs` — the pure `update(model, msg) -> commands`;
  `view.rs` — rendering; `keys.rs` — the one key table the help and hints come from.
- `client.rs` — loopback HTTP, the WebSocket subscription, `load_snapshot` and `run_cmd`.
- `api.rs` — hand-mirrored JSON shapes, checked against the real core in `tests/mock_core.rs`.
- `cli.rs` — the `bar`, `status` and `service` subcommands.
- `theme.rs` — the accent from Omarchy's `colors.toml`; `autostart.rs` — `systemctl --user`.
- `error_copy.rs` — the same sentences as the desktop app, keyed by the core's error code.

Colors are the terminal's sixteen ANSI colors on a `Reset` background, so
every Omarchy theme and its terminal transparency apply unchanged; the only
extra color is the theme's accent.
