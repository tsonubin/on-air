//! Header line and key-hint footer.

use super::hit::{HitMap, Target};
use crate::api::OutputPhase;
use crate::client::Connection;
use crate::keys::HINTS;
use crate::model::Model;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// The lamp and what is on air, then the connection state and the action
/// in flight.
fn left_spans(model: &Model) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let active = model
        .snapshot
        .active_output
        .as_ref()
        .filter(|_| model.service_on());
    match active {
        Some(active) => match active.state {
            Some(OutputPhase::Starting) => {
                spans.push(Span::styled(
                    " ◌ connecting ",
                    Style::new().fg(Color::Yellow),
                ));
                spans.push(Span::raw(format!("{} ", active.output.device_name)));
            }
            Some(OutputPhase::Failed) => {
                spans.push(Span::styled(
                    " ✗ failed ",
                    Style::new().fg(Color::Red).bold(),
                ));
                spans.push(Span::raw(format!("{} ", active.output.device_name)));
            }
            Some(OutputPhase::Live) | None => {
                spans.push(Span::styled(
                    " ● ON AIR ",
                    Style::new().fg(Color::Green).bold(),
                ));
                spans.push(Span::raw(format!(
                    "{} · {} ",
                    transport_label(&active.output.transport),
                    active.output.device_name
                )));
            }
        },
        None => spans.push(Span::styled(" ○ idle ", Style::new().dim())),
    }
    match model.connection {
        Connection::Closed | Connection::Reconnecting if model.snapshot.status.is_none() => {
            spans.push(Span::styled(
                " ✗ core unreachable ",
                Style::new().fg(Color::Red),
            ));
        }
        Connection::Closed => {
            spans.push(Span::styled(" unreachable ", Style::new().fg(Color::Red)));
        }
        Connection::Reconnecting | Connection::Connecting => {
            spans.push(Span::styled(
                " reconnecting ",
                Style::new().fg(Color::Yellow),
            ));
        }
        Connection::Open => {}
    }
    if let Some(pending) = &model.pending {
        spans.push(Span::styled(
            format!(" {}… ", pending.label),
            Style::new().fg(Color::Yellow),
        ));
    }
    spans
}

/// The right-aligned status, each span with the click target it stands for.
fn right_spans(model: &Model) -> Vec<(Span<'static>, Option<Target>)> {
    let mut spans = Vec::new();
    if model.service_on() {
        if let Some(pin) = &model.snapshot.pin {
            spans.push((Span::raw(format!(" PIN {pin} ")), Some(Target::StatusTag)));
        }
        if let Some(address) = model
            .snapshot
            .status
            .as_ref()
            .and_then(|s| s.lan_addresses.as_ref())
            .and_then(|a| a.first())
        {
            spans.push((
                Span::styled(format!(" {address} "), Style::new().dim()),
                Some(Target::StatusTag),
            ));
        }
        spans.push((Span::raw(" monitor on "), Some(Target::MonitorTag)));
    } else {
        spans.push((
            Span::styled(
                " monitor off — press m ",
                Style::new().fg(Color::Yellow).bold(),
            ),
            Some(Target::MonitorTag),
        ));
    }
    if let Some(autostart) = model.autostart {
        spans.push((
            Span::styled(
                if autostart {
                    " login ✓ "
                } else {
                    " login ✗ "
                },
                Style::new().dim(),
            ),
            None,
        ));
    }
    spans
}

/// Draw the header and record the click rects of its status and monitor tags.
pub(super) fn render_header(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let mut spans = left_spans(model);
    let right = right_spans(model);
    let left_width: usize = spans.iter().map(Span::width).sum();
    let right_width: usize = right.iter().map(|(span, _)| span.width()).sum();
    let gap = (area.width as usize)
        .saturating_sub(left_width + right_width)
        .max(1);
    spans.push(Span::raw(" ".repeat(gap)));

    let mut x = area.x as usize + left_width + gap;
    let mut status: Option<(usize, usize)> = None;
    for (span, target) in right {
        let width = span.width();
        match target {
            Some(Target::StatusTag) => {
                let start = status.map_or(x, |(start, _)| start);
                status = Some((start, x + width));
            }
            Some(target) => push_clipped(hits, area, x, x + width, target),
            None => {}
        }
        x += width;
        spans.push(span);
    }
    if let Some((start, end)) = status {
        push_clipped(hits, area, start, end, Target::StatusTag);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn push_clipped(hits: &mut HitMap, area: Rect, start: usize, end: usize, target: Target) {
    let limit = area.right() as usize;
    let (start, end) = (start.min(limit), end.min(limit));
    if end > start {
        hits.push(
            Rect::new(start as u16, area.y, (end - start) as u16, 1),
            target,
        );
    }
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
