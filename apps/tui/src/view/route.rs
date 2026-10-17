//! The Source and Destination panes.

use super::hit::{HitMap, Target};
use super::layout::{centered, locked_style, pane_block, row_rect, selection_style, truncate};
use crate::labels::input_labels;
use crate::model::{DestEntry, DestRow, DestState, Model, Pane};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn source_pane(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let focused = model.pane == Pane::Sources;
    let block = pane_block("Source", focused, model.accent);
    let labels = input_labels(&model.snapshot.inputs);
    if labels.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No capture devices found. Check PipeWire, then press r.",
                Style::new().add_modifier(Modifier::DIM),
            ))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let inner_width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = labels
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
    for index in 0..labels.len() {
        if let Some(rect) = row_rect(area, state.offset(), index) {
            hits.push(rect, Target::SourceRow(index));
        }
    }
}

/// The right-aligned tags of a Destination row: `⚇ pair` first, then its state.
fn dest_tags(entry: &DestEntry) -> Vec<(&'static str, Style)> {
    let mut tags = Vec::new();
    if entry.pair_badge {
        tags.push(("⚇ pair", Style::new()));
    }
    let state = match entry.state {
        DestState::Live => Some(("live", Style::new().fg(Color::Green))),
        DestState::Connecting => Some(("◌ connecting…", Style::new().fg(Color::Yellow))),
        DestState::Pairing => Some(("◌ pairing…", Style::new().fg(Color::Yellow))),
        DestState::Failed => Some(("✗ failed", Style::new().fg(Color::Red))),
        DestState::Pin => Some(("pin", Style::new().fg(Color::Yellow))),
        DestState::Connected => Some(("connected", Style::new().fg(Color::Green))),
        DestState::Paired => Some(("paired", Style::new().add_modifier(Modifier::DIM))),
        DestState::NotPaired => Some(("not paired", Style::new().add_modifier(Modifier::DIM))),
        DestState::Idle => None,
    };
    tags.extend(state);
    tags
}

pub(super) fn dest_pane(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let focused = model.pane == Pane::Outputs;
    let block = pane_block("Destination", focused, model.accent);
    let inner_width = area.width.saturating_sub(2) as usize;
    let dest_rows = model.dest_rows();
    let items: Vec<ListItem> = dest_rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| match row {
            DestRow::Header(label) => {
                ListItem::new(Line::from(Span::styled(*label, Style::new().bold())))
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
                let tags = dest_tags(entry);
                let tags_width: usize = tags.iter().map(|(tag, _)| tag.width()).sum::<usize>()
                    + 2 * tags.len().saturating_sub(1);
                let name = truncate(&entry.name, inner_width.saturating_sub(2 + tags_width + 1));
                let gap = inner_width
                    .saturating_sub(2 + name.width() + tags_width)
                    .max(1);
                let mut spans = vec![
                    Span::raw(marker),
                    Span::raw(name),
                    Span::raw(" ".repeat(gap)),
                ];
                for (index, (tag, style)) in tags.into_iter().enumerate() {
                    spans.push(Span::styled(
                        if index == 0 {
                            tag.to_string()
                        } else {
                            format!("  {tag}")
                        },
                        style,
                    ));
                }
                let style = if selected {
                    selection_style(model)
                } else if entry.placeholder {
                    Style::new().add_modifier(Modifier::DIM)
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
    if area.width >= 28 && area.height > 0 {
        let button = Rect::new(area.right() - 14, area.y, 13, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(" [r Refresh] ", Style::new().bold())),
            button,
        );
        hits.push(button, Target::Refresh);
    }
    for (index, row) in dest_rows.iter().enumerate() {
        if matches!(row, DestRow::Entry(_)) {
            if let Some(rect) = row_rect(area, state.offset(), index) {
                hits.push(rect, Target::DestRow(index));
            }
        }
    }
}

/// Replaces both route panes while the monitor is off.
pub(super) fn monitor_off_panel(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Monitor is off.", Style::new().bold())),
            Line::from(
                "Capture and streaming are stopped. Discovery and the phone remote stay online.",
            ),
            Line::from("Press m to turn it on."),
        ])
        .centered()
        .wrap(Wrap { trim: true }),
        centered(area, area.width, 4),
    );
}
