//! `on-air`: the terminal UI, plus `bar`, `status` and `service` subcommands
//! for Omarchy's bar module and menu rows. All of it is a client of the
//! `on-air-core` service; quitting never stops streaming.

use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;
use on_air_tui::client::{spawn_events, Client, DEFAULT_BASE};
use on_air_tui::model::Model;
use on_air_tui::theme::Theme;
use on_air_tui::update::{update, Cmd, Msg};
use on_air_tui::view::view;
use on_air_tui::{cli, error_copy};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

const USAGE: &str = "usage: on-air [--base URL]                 the terminal UI
       on-air bar [--base URL]             one Waybar-style JSON line for a bar module
       on-air status [--json] [--base URL] the core's state, for humans or scripts
       on-air service on|off|toggle        turn the LAN audio service on or off
       on-air --version | --help

The base URL defaults to http://127.0.0.1:47990, the local on-air-core.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Tui { base: String },
    Bar { base: String },
    Status { base: String, json: bool },
    Service { base: String, arg: String },
    Version,
    Help,
}

/// Hand-rolled: the surface is five forms and one flag.
pub fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut base = DEFAULT_BASE.to_string();
    let mut json = false;
    let mut positional: Vec<&str> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--base" => {
                base = iter
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("--base needs a URL\n{USAGE}"))?;
            }
            "--json" => json = true,
            "--version" | "-V" => return Ok(Invocation::Version),
            "--help" | "-h" => return Ok(Invocation::Help),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option {flag}\n{USAGE}"));
            }
            word => positional.push(word),
        }
    }
    match positional.as_slice() {
        [] => Ok(Invocation::Tui { base }),
        ["bar"] => Ok(Invocation::Bar { base }),
        ["status"] => Ok(Invocation::Status { base, json }),
        ["service", arg @ ("on" | "off" | "toggle")] => Ok(Invocation::Service {
            base,
            arg: arg.to_string(),
        }),
        ["service", ..] => Err(format!("service takes on, off or toggle\n{USAGE}")),
        other => Err(format!("unknown command {}\n{USAGE}", other.join(" "))),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let invocation = match parse_args(&args) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let result = match invocation {
        Invocation::Version => Ok(env!("CARGO_PKG_VERSION").to_string()),
        Invocation::Help => Ok(USAGE.to_string()),
        Invocation::Bar { base } => Ok(cli::bar(&Client::new(&base)).await),
        Invocation::Status { base, json } => cli::status(&Client::new(&base), json)
            .await
            .map_err(|error| error_copy::copy(&error)),
        Invocation::Service { base, arg } => cli::service(&Client::new(&base), &arg)
            .await
            .map_err(|error| error_copy::copy(&error)),
        Invocation::Tui { base } => {
            run_tui(Client::new(&base)).await;
            return;
        }
    };
    match result {
        Ok(text) => println!("{text}"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

/// The event loop. Every message goes through `update`; every command it
/// returns runs on its own task and feeds its replies back on the channel.
async fn run_tui(client: Client) {
    // A panic must not leave the terminal in raw mode on the alternate screen.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));
    let mut terminal = ratatui::init();

    let mut theme = Theme::load(&home());
    let mut model = Model::new();
    model.accent = theme.accent;
    if let Ok(size) = terminal.size() {
        model.size = (size.width, size.height);
    }

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Msg>();
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel();
    let events = spawn_events(client.base(), ws_tx);
    let mut keys = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let dispatch = |model: &mut Model, msg: Msg| -> bool {
        let cmds = update(model, msg);
        let quit = cmds.contains(&Cmd::Quit);
        for cmd in cmds {
            if cmd == Cmd::Quit {
                continue;
            }
            let client = client.clone();
            let tx = msg_tx.clone();
            tokio::spawn(async move {
                for reply in client.run_cmd(cmd).await {
                    if tx.send(reply).is_err() {
                        break;
                    }
                }
            });
        }
        quit
    };

    for cmd in [Cmd::Refresh, Cmd::QueryAutostart] {
        let client = client.clone();
        let tx = msg_tx.clone();
        tokio::spawn(async move {
            for reply in client.run_cmd(cmd).await {
                let _ = tx.send(reply);
            }
        });
    }

    let _ = terminal.draw(|frame| view(&model, frame));
    loop {
        let msg = tokio::select! {
            event = keys.next() => match event {
                Some(Ok(Event::Key(key))) => Msg::Key(key),
                Some(Ok(Event::Resize(w, h))) => Msg::Resize(w, h),
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => break,
            },
            incoming = ws_rx.recv() => match incoming {
                Some(incoming) => Msg::Ws(incoming),
                None => break,
            },
            reply = msg_rx.recv() => match reply {
                Some(reply) => reply,
                None => break,
            },
            _ = tick.tick() => {
                if theme.refresh() {
                    Msg::Accent(theme.accent)
                } else {
                    Msg::Tick
                }
            }
        };
        if dispatch(&mut model, msg) {
            break;
        }
        if terminal.draw(|frame| view(&model, frame)).is_err() {
            break;
        }
    }

    events.abort();
    ratatui::restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_is_the_tui_on_the_default_base() {
        assert_eq!(
            parse_args(&[]),
            Ok(Invocation::Tui {
                base: "http://127.0.0.1:47990".into()
            })
        );
    }

    #[test]
    fn subcommands_and_base_parse() {
        assert_eq!(
            parse_args(&args(&["bar", "--base", "http://10.0.0.2:47990"])),
            Ok(Invocation::Bar {
                base: "http://10.0.0.2:47990".into()
            })
        );
        assert_eq!(
            parse_args(&args(&["--base", "http://10.0.0.2:47990", "bar"])),
            Ok(Invocation::Bar {
                base: "http://10.0.0.2:47990".into()
            }),
            "--base is accepted anywhere"
        );
        assert_eq!(
            parse_args(&args(&["status", "--json"])),
            Ok(Invocation::Status {
                base: "http://127.0.0.1:47990".into(),
                json: true
            })
        );
        assert_eq!(
            parse_args(&args(&["service", "toggle"])),
            Ok(Invocation::Service {
                base: "http://127.0.0.1:47990".into(),
                arg: "toggle".into()
            })
        );
        assert_eq!(parse_args(&args(&["--version"])), Ok(Invocation::Version));
        assert_eq!(parse_args(&args(&["--help"])), Ok(Invocation::Help));
    }

    #[test]
    fn unknown_input_is_an_error_with_usage() {
        let err = parse_args(&args(&["frobnicate"])).unwrap_err();
        assert!(err.contains("usage:"), "{err}");
        assert!(parse_args(&args(&["service", "maybe"])).is_err());
        assert!(parse_args(&args(&["service"])).is_err());
        assert!(
            parse_args(&args(&["--base"])).is_err(),
            "--base needs a value"
        );
        assert!(parse_args(&args(&["--frob"])).is_err());
    }
}
