//! Layout helpers shared by the panes: text truncation, pane chrome, centring.

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
    if model.locked() {
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
