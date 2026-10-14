//! Layout helpers shared by the panes: text truncation, pane chrome, centring.

use super::{COMPACT_BELOW_ROWS, STACK_BELOW_COLUMNS};
use crate::model::Model;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Cut `text` to `width` display cells, ending in `…` when anything was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Where each part of the console goes. Rows that have nothing to show are
/// zero-height.
pub(super) struct Areas {
    pub header: Rect,
    pub cd: Rect,
    pub error: Rect,
    pub notice: Rect,
    pub source: Rect,
    pub dest: Rect,
    pub mixer: Rect,
    pub hints: Rect,
}

/// The console: header, CD strip, error and notice lines, Source and
/// Destination (side by side, or stacked when narrow), the mixer, hints.
pub(super) fn areas(model: &Model, area: Rect) -> Areas {
    let cd_rows = u16::from(model.snapshot.cd.as_ref().is_some_and(|cd| cd.present));
    let mixer_rows = if area.height < COMPACT_BELOW_ROWS {
        4
    } else {
        7
    };
    let [header, cd, error, notice, route, mixer, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(cd_rows),
        Constraint::Length(u16::from(model.error.is_some())),
        Constraint::Length(u16::from(model.notice.is_some())),
        Constraint::Min(0),
        Constraint::Length(mixer_rows),
        Constraint::Length(1),
    ])
    .areas(area);
    let [source, dest] = if area.width < STACK_BELOW_COLUMNS {
        Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(route)
    } else {
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(route)
    };
    Areas {
        header,
        cd,
        error,
        notice,
        source,
        dest,
        mixer,
        hints,
    }
}

/// The one-cell-high rect of list item `index` inside a bordered pane
/// whose list is scrolled to `offset`, if it is on screen.
pub(super) fn row_rect(pane: Rect, offset: usize, index: usize) -> Option<Rect> {
    let inner = Block::bordered().inner(pane);
    let line = u16::try_from(index.checked_sub(offset)?).ok()?;
    (line < inner.height).then(|| Rect::new(inner.x, inner.y + line, inner.width, 1))
}

pub(super) fn pane_block<'a>(title: &'a str, focused: bool, accent: Color) -> Block<'a> {
    let block = Block::bordered().title(format!(" {title} "));
    if focused {
        block.border_style(Style::new().fg(accent))
    } else {
        block
    }
}

pub(super) fn selection_style(model: &Model) -> Style {
    Style::new()
        .fg(model.accent)
        .add_modifier(Modifier::REVERSED)
}

pub(super) fn locked_style(model: &Model, style: Style) -> Style {
    if model.locked() || model.core_down {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}
pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(vertical);
    horizontal
}
