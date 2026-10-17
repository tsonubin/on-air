//! `on-air`: the terminal UI, plus `bar`, `status`, `monitor` and `service`
//! subcommands for Omarchy's bar module and menu rows. All of it is a client
//! of the `on-air-core` service; quitting never stops streaming.

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream};
use futures_util::StreamExt;
use on_air_tui::client::{spawn_events, Client, DEFAULT_BASE};
use on_air_tui::model::Model;
use on_air_tui::service::{self, ServiceVerb};
use on_air_tui::theme::Theme;
use on_air_tui::update::{update, Cmd, Msg};
use on_air_tui::view::{view, HitMap};
use on_air_tui::{cli, error_copy};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

mod input;

const USAGE: &str = "usage: on-air [--base URL]                 the terminal UI
       on-air bar [--base URL]             one Waybar-style JSON line for a bar module
       on-air status [--json] [--base URL] the core's state, for humans or scripts
       on-air monitor on|off|toggle        turn LAN audio capture and streaming on or off
       on-air service start|stop|restart   control the on-air-core systemd user service
       on-air audio check [--json]         benchmark audio and recommend a buffer
       on-air audio buffer [default|50|100|200|400]  read or set capture buffering
       on-air firewall [allow]            show firewall help or authorize a scoped repair
       on-air --version | --help

The base URL defaults to http://127.0.0.1:47990, the local on-air-core.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Tui {
        base: String,
    },
    Bar {
        base: String,
    },
    Status {
        base: String,
        json: bool,
    },
    Monitor {
        base: String,
        arg: String,
    },
    Service {
        verb: ServiceVerb,
    },
    Version,
    Audio {
        base: String,
        action: String,
        value: Option<u32>,
        json: bool,
    },
    Firewall {
        base: String,
        allow: bool,
    },
    Help,
}

/// Hand-rolled: the surface is six forms and two flags.
pub fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut base = DEFAULT_BASE.to_string();
    let mut base_given = false;
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
                base_given = true;
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
        ["firewall"] => Ok(Invocation::Firewall { base, allow: false }),
        ["firewall", "allow"] => Ok(Invocation::Firewall { base, allow: true }),
        ["audio", "check"] => Ok(Invocation::Audio {
            base,
            action: "check".into(),
            value: None,
            json,
        }),
        ["audio", "buffer"] => Ok(Invocation::Audio {
            base,
            action: "buffer".into(),
            value: None,
            json,
        }),
        ["audio", "buffer", value] => {
            let buffer = if *value == "default" {
                Some(0)
            } else {
                value.parse::<u32>().ok()
            };
            match buffer {
                Some(value @ (0 | 50 | 100 | 200 | 400)) => Ok(Invocation::Audio {
                    base,
                    action: "buffer".into(),
                    value: Some(value),
                    json,
                }),
                _ => Err("buffer must be default, 50, 100, 200 or 400 ms".into()),
            }
        }
        ["monitor", arg @ ("on" | "off" | "toggle")] => Ok(Invocation::Monitor {
            base,
            arg: arg.to_string(),
        }),
        ["monitor", ..] => Err(format!("monitor takes on, off or toggle\n{USAGE}")),
        ["service", arg @ ("on" | "off" | "toggle")] => Err(format!(
            "`on-air service {arg}` is now `on-air monitor {arg}`\n{USAGE}"
        )),
        ["service", word] => match ServiceVerb::parse(word) {
            Some(_) if base_given => Err(format!(
                "--base does not apply to service: systemd manages the local core\n{USAGE}"
            )),
            Some(verb) => Ok(Invocation::Service { verb }),
            None => Err(format!("service takes start, stop or restart\n{USAGE}")),
        },
        ["service", ..] => Err(format!("service takes start, stop or restart\n{USAGE}")),
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
        Invocation::Monitor { base, arg } => cli::monitor(&Client::new(&base), &arg)
            .await
            .map_err(|error| error_copy::copy(&error)),
        Invocation::Service { verb } => service::control(verb).await,
        Invocation::Audio {
            base,
            action,
            value,
            json,
        } => cli::audio(&Client::new(&base), &action, value, json)
            .await
            .map_err(|error| error_copy::copy(&error)),
        Invocation::Firewall { base, allow } => cli::firewall(&Client::new(&base), allow)
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
    let mut terminal = ratatui::init();
    // A panic must not leave the terminal capturing the mouse, in raw mode or
    // on the alternate screen. This hook wraps the one `ratatui::init`
    // installs, so capture is off before ratatui restores the rest.
    let ratatui_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
        ratatui_hook(info);
    }));
    let mouse = crossterm::execute!(std::io::stdout(), EnableMouseCapture).is_ok();

    let mut theme = Theme::load(&home());
    let mut model = Model::new();
    model.accent = theme.accent;
    model.base = client.base().to_string();
    model.mouse = mouse;
    if let Ok(size) = terminal.size() {
        model.size = (size.width, size.height);
    }

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Msg>();
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel();
    let events = spawn_events(client.base(), ws_tx);
    let mut keys = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut frame_tick = tokio::time::interval(FRAME);
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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

    let mut hits = HitMap::default();
    let mut drag = input::DragState::default();
    let mut draw = |model: &Model, hits: &mut HitMap| {
        terminal.draw(|frame| *hits = view(model, frame)).is_ok()
    };
    // The frame interval fires at once, so this is the first draw.
    let mut dirty = true;
    loop {
        let msg = tokio::select! {
            event = keys.next() => match event {
                Some(Ok(Event::Key(key))) => Msg::Key(key),
                Some(Ok(Event::Mouse(event))) => {
                    // Rows are indices into what is on screen: catch the
                    // screen up with the model before resolving the click.
                    if dirty {
                        dirty = false;
                        if !draw(&model, &mut hits) {
                            break;
                        }
                    }
                    match drag.mouse_msg(&hits, event) {
                        Some(msg) => msg,
                        None => continue,
                    }
                }
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
            _ = frame_tick.tick() => Msg::Frame,
            _ = tick.tick() => {
                if theme.refresh() {
                    Msg::Accent(theme.accent)
                } else {
                    Msg::Tick
                }
            }
        };
        // Keys and clicks draw at once; everything else waits for the next
        // frame, so a burst of level events costs one draw.
        let now = matches!(msg, Msg::Key(_) | Msg::Mouse { .. });
        let frame = msg == Msg::Frame;
        let level = (model.meter.rms_db, model.meter.peak_db);
        if dispatch(&mut model, msg) {
            break;
        }
        dirty |= !frame || level != (model.meter.rms_db, model.meter.peak_db);
        if now || (frame && dirty) {
            dirty = false;
            if !draw(&model, &mut hits) {
                break;
            }
        }
    }

    events.abort();
    restore();
}

/// One frame of the level meter's animation and of redraw throttling.
const FRAME: Duration = Duration::from_millis(50);

/// Hand the terminal back: mouse capture off before raw mode and the
/// alternate screen.
fn restore() {
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
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
            parse_args(&args(&["monitor", "toggle"])),
            Ok(Invocation::Monitor {
                base: "http://127.0.0.1:47990".into(),
                arg: "toggle".into()
            })
        );
        for (word, verb) in [
            ("start", ServiceVerb::Start),
            ("stop", ServiceVerb::Stop),
            ("restart", ServiceVerb::Restart),
        ] {
            assert_eq!(
                parse_args(&args(&["service", word])),
                Ok(Invocation::Service { verb })
            );
        }
        assert_eq!(parse_args(&args(&["--version"])), Ok(Invocation::Version));
        assert_eq!(parse_args(&args(&["--help"])), Ok(Invocation::Help));
    }

    #[test]
    fn unknown_input_is_an_error_with_usage() {
        let err = parse_args(&args(&["frobnicate"])).unwrap_err();
        assert!(err.contains("usage:"), "{err}");
        assert!(parse_args(&args(&["service", "maybe"])).is_err());
        assert!(parse_args(&args(&["service"])).is_err());
        assert!(parse_args(&args(&["monitor", "maybe"])).is_err());
        assert!(parse_args(&args(&["monitor"])).is_err());
    }

    #[test]
    fn the_old_service_switch_points_at_monitor() {
        for arg in ["on", "off", "toggle"] {
            let err = parse_args(&args(&["service", arg])).unwrap_err();
            assert!(err.contains(&format!("on-air monitor {arg}")), "{err}");
        }
    }

    #[test]
    fn service_verbs_reject_base() {
        let err = parse_args(&args(&[
            "service",
            "restart",
            "--base",
            "http://10.0.0.2:47990",
        ]))
        .unwrap_err();
        assert!(err.contains("--base"), "{err}");
        assert!(
            parse_args(&args(&["--base"])).is_err(),
            "--base needs a value"
        );
        assert!(parse_args(&args(&["--frob"])).is_err());
    }
}
