# on-air terminal UI

`on-air` is the Linux control surface: a ratatui terminal UI over the
`on-air-core` service on `http://127.0.0.1:47990`, plus `bar`, `status`,
`monitor`, `service`, `audio` and `firewall` subcommands for Omarchy's bar,
menu and diagnostics. It owns no audio; quitting it never stops streaming.
Keys, mouse, the Omarchy launcher entry, keybinding, menu rows and bar module
are documented in [`docs/omarchy.md`](../../docs/omarchy.md).

```bash
# dev loop: a hardware-free core in one terminal, the UI in another
ON_AIR_MOCK=1 PORT=47990 cargo run -p on-air-core --bin on-air-core
cargo run -p on-air-tui                            # the UI
cargo run -p on-air-tui -- status [--json]         # the core's state
cargo run -p on-air-tui -- monitor on|off|toggle   # audio capture and streaming
cargo run -p on-air-tui -- service start|stop|restart   # the systemd user unit
cargo run -p on-air-tui -- audio check [--json]   # benchmark capture buffering
cargo run -p on-air-tui -- firewall [allow]       # show or repair UFW access

cargo test -p on-air-tui                      # unit tests + the mock core in-process
cargo clippy --locked -p on-air-tui --all-targets -- -D warnings
```

`--base URL` points the UI, `bar`, `status` and `monitor` at another core.
`service` always talks to the local systemd unit.

## Layout of `src/`

Elm-style: a model, a pure update function, a pure view, and I/O at the edges.

- `main.rs`: argument parsing, terminal lifecycle, the event loop; `input.rs`
  turns crossterm events into messages.
- `model.rs`: the whole UI state. `update.rs`: `update(model, msg) -> commands`.
  `view/`: rendering, returning a `HitMap` so mouse clicks map back to
  messages. `keys.rs`: the one key table the help and hints come from.
- `client.rs`: loopback HTTP, the WebSocket subscription, `load_snapshot`
  and `run_cmd`. `api.rs`: the JSON shapes, checked against the real core in
  `tests/mock_core.rs`.
- `cli.rs`: the `bar`, `status` and `monitor` subcommands; `service.rs` and
  `autostart.rs`: `systemctl --user`.
- `theme.rs`: the accent from Omarchy's `colors.toml`; `labels.rs` and
  `error_copy.rs`: the same device names and error sentences as the desktop
  app, keyed by the core's error code; `handoff.rs`: opening Bluetooth settings.

Colors are the terminal's sixteen ANSI colors on a `Reset` background, so
every Omarchy theme and its terminal transparency apply unchanged; the only
extra color is the theme's accent.
