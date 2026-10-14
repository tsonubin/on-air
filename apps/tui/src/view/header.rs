//! Header line and key-hint footer.

use crate::keys::HINTS;
use crate::model::Model;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(super) fn header_line(model: &Model) -> Paragraph<'_> {
    let mut spans = vec![Span::styled(" on-air ", Style::new().bold())];
    match model.connection {
        crate::client::Connection::Closed | crate::client::Connection::Reconnecting
            if model.snapshot.status.is_none() =>
        {
            spans.push(Span::styled(
                " core unreachable ",
                Style::new().fg(Color::Red),
            ));
        }
        crate::client::Connection::Closed => {
            spans.push(Span::styled(" unreachable ", Style::new().fg(Color::Red)));
        }
        crate::client::Connection::Reconnecting | crate::client::Connection::Connecting => {
            spans.push(Span::styled(
                " reconnecting ",
                Style::new().fg(Color::Yellow),
            ));
        }
        crate::client::Connection::Open => {}
    }
    if !model.service_on() {
        spans.push(Span::styled(
            " monitor off — press m ",
            Style::new().fg(Color::Yellow).bold(),
        ));
    } else {
        match &model.snapshot.active_output {
            Some(active) if active.is_live() => {
                spans.push(Span::styled(
                    " ● live ",
                    Style::new().fg(Color::Green).bold(),
                ));
                spans.push(Span::raw(format!(
                    "{} · {} ",
                    transport_label(&active.output.transport),
                    active.output.device_name
                )));
            }
            Some(active) => {
                spans.push(Span::styled(" ◌ starting ", Style::new().fg(Color::Yellow)));
                spans.push(Span::raw(format!("{} ", active.output.device_name)));
            }
            None => spans.push(Span::styled(" ○ idle ", Style::new().dim())),
        }
        if let Some(pending) = &model.pending {
            spans.push(Span::styled(
                format!(" {}… ", pending.label),
                Style::new().fg(Color::Yellow),
            ));
        }
        if let Some(pin) = &model.snapshot.pin {
            spans.push(Span::raw(format!(" PIN {pin} ")));
        }
        if let Some(address) = model
            .snapshot
            .status
            .as_ref()
            .and_then(|s| s.lan_addresses.as_ref())
            .and_then(|a| a.first())
        {
            spans.push(Span::styled(format!(" {address} "), Style::new().dim()));
        }
        spans.push(Span::raw(" monitor on "));
    }
    if let Some(autostart) = model.autostart {
        spans.push(Span::styled(
            if autostart {
                " login ✓ "
            } else {
                " login ✗ "
            },
            Style::new().dim(),
        ));
    }
    Paragraph::new(Line::from(spans))
}

pub(super) fn transport_label(transport: &str) -> &'static str {
    crate::model::TRANSPORTS
        .iter()
        .find(|(id, _)| *id == transport)
        .map(|(_, label)| *label)
        .unwrap_or("Output")
}

pub(super) fn hint_line() -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (key, what) in HINTS {
        spans.push(Span::styled(format!(" {key} "), Style::new().bold()));
        spans.push(Span::styled(format!("{what} "), Style::new().dim()));
    }
    Paragraph::new(Line::from(spans))
}
