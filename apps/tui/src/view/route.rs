//! The Sources and Outputs panes.

use super::layout::{locked_style, pane_block, selection_style, truncate};
use crate::labels::input_labels;
use crate::model::{DestRow, DestState, Model, Pane};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

pub(super) fn sources_pane(model: &Model, frame: &mut Frame, area: Rect) {
    let focused = model.pane == Pane::Sources;
    let block = pane_block("Sources", focused, model.accent);
    let inner_width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = input_labels(&model.snapshot.inputs)
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let selected = focused && index == model.source_index;
            let active = model.snapshot.active_input.as_deref() == Some(row.name.as_str());
            let marker = if selected { "> " } else { "  " };
            let label = truncate(&row.label, inner_width.saturating_sub(4));
            let mut spans = vec![Span::raw(marker), Span::raw(label)];
            if active {
                spans.push(Span::styled(" ●", Style::new().fg(Color::Green)));
            }
            let style = if selected {
                selection_style(model)
            } else {
                Style::new()
            };
            ListItem::new(Line::from(spans)).style(locked_style(model, style))
        })
        .collect();
    // The state only scrolls the list to the cursor; the marker and
    // highlight are drawn by the items themselves.
    let mut state = ListState::default().with_selected(Some(model.source_index));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

pub(super) fn outputs_pane(model: &Model, frame: &mut Frame, area: Rect) {
    let focused = model.pane == Pane::Outputs;
    let block = pane_block("Outputs", focused, model.accent);
    let inner_width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = model
        .dest_rows()
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| match row {
            DestRow::Header(label) => {
                ListItem::new(Line::from(Span::styled(label, Style::new().bold())))
                    .style(locked_style(model, Style::new()))
            }
            DestRow::Empty(text) => ListItem::new(Line::from(Span::styled(
                format!("  {}", truncate(text, inner_width.saturating_sub(2))),
                Style::new().add_modifier(Modifier::DIM),
            )))
            .style(locked_style(model, Style::new())),
            DestRow::Entry(entry) => {
                let selected = focused && row_index == model.output_index;
                let marker = if selected { "> " } else { "  " };
                let suffix = if matches!(entry.state, DestState::Pin | DestState::NotPaired) {
                    " (pair)"
                } else {
                    ""
                };
                let is_live = entry.state == DestState::Live;
                let budget =
                    inner_width.saturating_sub(2 + suffix.len() + if is_live { 7 } else { 0 });
                let mut spans = vec![
                    Span::raw(marker),
                    Span::raw(format!("{}{suffix}", truncate(&entry.name, budget))),
                ];
                if is_live {
                    spans.push(Span::styled(" ● live", Style::new().fg(Color::Green)));
                }
                let style = if selected {
                    selection_style(model)
                } else {
                    Style::new()
                };
                ListItem::new(Line::from(spans)).style(locked_style(model, style))
            }
        })
        .collect();
    // The state only scrolls the list to the cursor; the marker and
    // highlight are drawn by the items themselves.
    let mut state = ListState::default().with_selected(Some(model.output_index));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}
