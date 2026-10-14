//! The Mixer pane.

use super::layout::{locked_style, pane_block, selection_style};
use crate::model::{MixerRow, Model, Pane};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

const EQ_BANDS: [&str; 5] = ["60", "250", "1k", "4k", "12k"];
const EQ_CELLS: usize = 9;
const VOLUME_CELLS: usize = 10;

pub(super) fn bar(filled: usize, cells: usize) -> String {
    let filled = filled.min(cells);
    format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled))
}

pub(super) fn eq_band(gain_db: f32) -> String {
    let (lo, hi) = crate::update::EQ_RANGE_DB;
    let position = ((gain_db - lo) / (hi - lo) * (EQ_CELLS - 1) as f32).round() as usize;
    (0..EQ_CELLS)
        .map(|cell| {
            if cell == position.min(EQ_CELLS - 1) {
                '█'
            } else {
                '░'
            }
        })
        .collect()
}

pub(super) fn mixer_pane(model: &Model, frame: &mut Frame, area: Rect) {
    let focused = model.pane == Pane::Mixer;
    let block = pane_block("Mixer", focused, model.accent);
    let snapshot = &model.snapshot;
    let rows = MixerRow::ALL.iter().map(|row| {
        let text = match row {
            MixerRow::Volume => format!(
                "Volume   {}  {:>3}",
                bar(
                    snapshot.volume as usize / (100 / VOLUME_CELLS),
                    VOLUME_CELLS
                ),
                snapshot.volume
            ),
            MixerRow::Eq(band) => format!(
                "EQ {:<4} {}  {:+.1} dB",
                EQ_BANDS[*band],
                eq_band(snapshot.eq[*band]),
                snapshot.eq[*band]
            ),
            MixerRow::InputRate => match &snapshot.rates {
                Some(rates) => format!("In {} Hz", rates.input.sample_rate_hz),
                None => "In —".to_string(),
            },
            MixerRow::OutputRate => match &snapshot.rates {
                Some(rates) => format!("Out {} Hz", rates.output.sample_rate_hz),
                None => "Out —".to_string(),
            },
        };
        let selected = focused && *row == model.mixer_row;
        let marker = if selected { "> " } else { "  " };
        let style = if selected {
            selection_style(model)
        } else {
            Style::new()
        };
        ListItem::new(Line::from(vec![Span::raw(marker), Span::raw(text)]))
            .style(locked_style(model, style))
    });
    // The state only scrolls the list to the cursor; the marker and
    // highlight are drawn by the items themselves.
    let mut state = ListState::default().with_selected(Some(
        MixerRow::ALL
            .iter()
            .position(|row| *row == model.mixer_row)
            .unwrap_or(0),
    ));
    frame.render_stateful_widget(List::new(rows).block(block), area, &mut state);
}
