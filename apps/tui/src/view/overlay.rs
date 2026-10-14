//! The help, pairing and CD overlays.

use super::layout::centered;
use crate::keys::KEYS;
use crate::model::Model;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// The key table, then the mouse rows, so the help still fits 100×30.
pub(super) fn help_overlay(frame: &mut Frame, area: Rect, mouse: bool) -> Rect {
    let row = |keys: &str, help: Span<'static>| {
        Line::from(vec![
            Span::styled(format!(" {keys:<16} "), Style::new().bold()),
            help,
        ])
    };
    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|binding| row(binding.keys, Span::raw(binding.help)))
        .collect();
    lines.push(row(
        "Mouse",
        if mouse {
            Span::raw("Click rows, buttons and tags; drag or wheel faders")
        } else {
            Span::styled("mouse off", Style::new().dim())
        },
    ));
    if mouse {
        lines.push(row(
            "",
            Span::raw("Hold Shift while dragging to select text."),
        ));
    }
    lines.push(Line::from(Span::styled(
        " any key or a click outside closes ",
        Style::new().dim(),
    )));
    let width = area.width.min(80);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Keys ")),
        rect,
    );
    rect
}

pub(super) fn pin_overlay(
    frame: &mut Frame,
    area: Rect,
    target: &crate::model::PairTarget,
    input: &str,
    error: Option<&str>,
    pairing: bool,
) -> Rect {
    let width = area.width.min(60);
    let rect = centered(area, width, 7);
    frame.render_widget(Clear, rect);
    let prompt = if target.needs_pin() {
        "Enter the PIN shown by the speaker, then press Enter."
    } else {
        "Press Enter to pair, Esc to cancel."
    };
    let shown = if target.needs_pin() {
        format!("PIN: {input}▏")
    } else {
        String::new()
    };
    let mut lines = vec![
        Line::from(prompt),
        Line::from(Span::styled(shown, Style::new().bold())),
    ];
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    if pairing {
        lines.push(Line::from(Span::styled(
            "Pairing…",
            Style::new().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Pair {} ", target.name()))),
        rect,
    );
    rect
}

pub(super) fn track_overlay(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    error: Option<&str>,
) -> Rect {
    let rect = centered(area, area.width.min(44), 6);
    frame.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from("Type a track number, then press Enter."),
        Line::from(Span::styled(
            format!("Track: {input}▏"),
            Style::new().bold(),
        )),
    ];
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" Go to track ")),
        rect,
    );
    rect
}

pub(super) fn eject_overlay(frame: &mut Frame, area: Rect) -> Rect {
    let rect = centered(area, area.width.min(36), 5);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Eject the disc?"),
            Line::from(Span::styled("y eject   n cancel", Style::new().dim())),
        ])
        .block(Block::bordered().title(" Eject ")),
        rect,
    );
    rect
}

/// The core's default port, for a base URL that does not parse.
const DEFAULT_PORT: u16 = 47990;

/// The port the core answers on, from its base URL.
fn core_port(base: &str) -> u16 {
    reqwest::Url::parse(base)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(DEFAULT_PORT)
}

pub(super) fn status_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let status = model.snapshot.status.as_ref();
    let port = core_port(&model.base);
    let yes_no = |value: Option<bool>| match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    let on_off = |value: Option<bool>| match value {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    };
    let mut lines = vec![
        Line::from(format!(
            "Version: {}   Port: {port}",
            status.map_or("unknown", |s| s.version.as_str())
        )),
        Line::from("Addresses:"),
    ];
    match status.and_then(|s| s.lan_addresses.as_ref()) {
        Some(addresses) if !addresses.is_empty() => lines.extend(
            addresses
                .iter()
                .map(|address| Line::from(format!("  {address}:{port}"))),
        ),
        _ => lines.push(Line::from("  none found")),
    }
    lines.extend([
        Line::from(format!(
            "AirPlay: {}",
            model.snapshot.airplay_mode.as_deref().unwrap_or("unknown")
        )),
        Line::from(if model.service_on() {
            "monitor on"
        } else {
            "monitor off"
        }),
        Line::from(format!(
            "sleep inhibited: {}",
            yes_no(status.and_then(|s| s.sleep_inhibited))
        )),
        Line::from(format!("login start: {}", on_off(model.autostart))),
        Line::from(format!("Core: {}", model.base)),
        Line::from(Span::styled(" Esc or i closes ", Style::new().dim())),
    ]);
    let rect = centered(
        area,
        area.width.min(60),
        (lines.len() as u16 + 2).min(area.height),
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Status ")),
        rect,
    );
    rect
}
