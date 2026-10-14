//! The help, pairing and CD overlays.

use super::layout::centered;
use crate::keys::KEYS;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn help_overlay(frame: &mut Frame, area: Rect) {
    let width = area.width.min(80);
    let height = (KEYS.len() as u16 + 3).min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|binding| {
            Line::from(vec![
                Span::styled(format!(" {:<16} ", binding.keys), Style::new().bold()),
                Span::raw(binding.help),
            ])
        })
        .chain(std::iter::once(Line::from(Span::styled(
            " any key closes ",
            Style::new().dim(),
        ))))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Keys ")),
        rect,
    );
}

pub(super) fn pin_overlay(
    frame: &mut Frame,
    area: Rect,
    target: &crate::model::PairTarget,
    input: &str,
    error: Option<&str>,
) {
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
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Pair {} ", target.name()))),
        rect,
    );
}

pub(super) fn track_overlay(frame: &mut Frame, area: Rect, input: &str, error: Option<&str>) {
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
}

pub(super) fn eject_overlay(frame: &mut Frame, area: Rect) {
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
}
