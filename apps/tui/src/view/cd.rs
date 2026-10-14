//! The CD transport line.

use crate::model::Model;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(super) fn cd_line(model: &Model) -> Paragraph<'_> {
    let Some(cd) = &model.snapshot.cd else {
        return Paragraph::new("");
    };
    let glyph = if cd.playing { "▶" } else { "⏸" };
    let title = cd
        .title
        .clone()
        .or_else(|| cd.album.clone())
        .unwrap_or_default();
    Paragraph::new(Line::from(vec![
        Span::styled(format!(" {glyph} Audio CD "), Style::new().fg(Color::Cyan)),
        Span::raw(format!(" {:02}/{:02}  ", cd.track, cd.track_count)),
        Span::raw(title),
        Span::styled(
            format!("  {}/{}", clock(cd.position_ms), clock(cd.duration_ms)),
            Style::new().dim(),
        ),
    ]))
}

pub(super) fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
