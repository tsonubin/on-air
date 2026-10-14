//! The CD strip: transport buttons, track, progress bar, clock and eject.

use super::hit::{CdButton, HitMap, Target};
use super::layout::truncate;
use crate::model::Model;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const CLOCK_WIDTH: u16 = 13;
const BAR_WIDTH: u16 = 16;
const BAR_MIN_WIDTH: u16 = 4;
const LABEL_MIN_WIDTH: u16 = 12;
const EJECT_WIDTH: u16 = 3;

pub(super) fn cd_strip(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let Some(cd) = model.snapshot.cd.as_ref().filter(|cd| cd.present) else {
        return;
    };
    let cyan = Style::new().fg(Color::Cyan);
    let play = if cd.playing { "❚❚" } else { "▶" };
    let buttons = [
        ("⏮", CdButton::Prev),
        (play, CdButton::PlayPause),
        ("⏭", CdButton::Next),
    ];
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    for (glyph, button) in buttons {
        let width = glyph.width() as u16 + 1;
        spans.push(Span::styled(format!("{glyph} "), cyan));
        hits.push(Rect::new(x, area.y, width, 1), Target::CdButton(button));
        x += width;
    }
    spans.push(Span::raw(" "));
    x += 1;

    // What is left after the fixed parts goes to the bar first, down to its
    // minimum, then to the label.
    let fixed = (x - area.x) + EJECT_WIDTH + CLOCK_WIDTH + 4;
    let spare = area.width.saturating_sub(fixed);
    let bar_width = BAR_WIDTH.min(spare.saturating_sub(LABEL_MIN_WIDTH).max(BAR_MIN_WIDTH));
    let label_width = spare.saturating_sub(bar_width);

    let title = match (&cd.album, &cd.title) {
        (Some(album), Some(title)) => format!("{album} — {title}"),
        (Some(text), None) | (None, Some(text)) => text.clone(),
        (None, None) => String::new(),
    };
    let label = truncate(
        &format!("{:02}/{:02}  {title}", cd.track, cd.track_count),
        label_width.saturating_sub(2) as usize,
    );
    let padded = format!(
        "{label:<width$}  ",
        width = label_width.saturating_sub(2) as usize
    );
    x += padded.width() as u16;
    spans.push(Span::raw(padded));

    let filled = (cd.position_ms.min(cd.duration_ms) * u64::from(bar_width) + cd.duration_ms / 2)
        .checked_div(cd.duration_ms)
        .unwrap_or(0) as usize;
    spans.push(Span::styled("━".repeat(filled), cyan));
    spans.push(Span::styled(
        "─".repeat(usize::from(bar_width).saturating_sub(filled)),
        Style::new().dim(),
    ));
    hits.push(Rect::new(x, area.y, bar_width, 1), Target::CdProgress);

    spans.push(Span::styled(
        format!("  {} / {}", clock(cd.position_ms), clock(cd.duration_ms)),
        Style::new().dim(),
    ));
    spans.push(Span::raw("  "));
    let eject_x = area.x + area.width.saturating_sub(EJECT_WIDTH);
    spans.push(Span::styled("⏏", cyan));
    hits.push(
        Rect::new(eject_x, area.y, EJECT_WIDTH.min(area.width), 1),
        Target::CdButton(CdButton::Eject),
    );
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
