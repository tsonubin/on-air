//! The Mixer pane: vertical volume and EQ faders, the level meter and the
//! sample rates; one line per control when the window is short.

use super::hit::{HitMap, Target};
use super::layout::{locked_style, pane_block, row_rect, selection_style};
use crate::model::METER_FLOOR_DB;
use crate::model::{Meter, MixerRow, Model, Pane};
use crate::update::EQ_RANGE_DB;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

const EQ_BANDS: [&str; 5] = ["60", "250", "1k", "4k", "12k"];
const EQ_CELLS: usize = 9;
const VOLUME_CELLS: usize = 10;
/// Pane height (with borders) below which the compact form is used.
const COMPACT_BELOW_HEIGHT: u16 = 7;
const FADER_ROW: u16 = 1;
const VOLUME_WIDTH: u16 = 5;
const BAND_WIDTH: u16 = 5;
/// Wide enough for "-12.0 dB", centred on the fader.
const DB_LABEL_WIDTH: u16 = BAND_WIDTH + 4;
const RATE_WIDTH: u16 = 14;
const METER_MAX_WIDTH: u16 = 24;
const METER_MIN_WIDTH: u16 = 4;
const EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

pub(super) fn bar(filled: usize, cells: usize) -> String {
    let filled = filled.min(cells);
    format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled))
}

pub(super) fn eq_band(gain_db: f32) -> String {
    let (lo, hi) = EQ_RANGE_DB;
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

/// The value under row `offset` of a vertical fader `extent` cells tall:
/// the top is `max`, the bottom `min`, rounded to the nearest `step`.
pub fn fader_value(offset: u16, extent: u16, min: f32, max: f32, step: f32) -> f32 {
    if extent <= 1 {
        return max;
    }
    let offset = offset.min(extent - 1);
    let raw = max - f32::from(offset) / f32::from(extent - 1) * (max - min);
    ((raw / step).round() * step).clamp(min, max)
}

/// The fader row (0 = top) a value sits on; the inverse of `fader_value`.
fn fader_offset(value: f32, extent: u16, min: f32, max: f32) -> u16 {
    let fraction = ((max - value) / (max - min)).clamp(0.0, 1.0);
    (fraction * f32::from(extent.saturating_sub(1))).round() as u16
}

fn meter_color(db: f32) -> Color {
    if db >= -3.0 {
        Color::Red
    } else if db >= -12.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// The meter bar as one styled cell per column: eighth blocks up to the
/// RMS level and a `│` tick at the held peak.
fn meter_cells(meter: &Meter, width: usize) -> Vec<Span<'static>> {
    let scale = -METER_FLOOR_DB;
    let fraction = |db: f32| ((db - METER_FLOOR_DB) / scale).clamp(0.0, 1.0);
    let eighths = (fraction(meter.rms_db) * width as f32 * 8.0).round() as usize;
    let peak_cell = (meter.peak_db > METER_FLOOR_DB)
        .then(|| (fraction(meter.peak_db) * (width.saturating_sub(1)) as f32).round() as usize);
    (0..width)
        .map(|cell| {
            let db = METER_FLOOR_DB + scale * (cell as f32 + 0.5) / width as f32;
            let style = Style::new().fg(meter_color(db));
            if peak_cell == Some(cell) {
                return Span::styled("│", style);
            }
            let fill = eighths.saturating_sub(cell * 8).min(8);
            match fill {
                0 => Span::raw(" "),
                n => Span::styled(EIGHTHS[n - 1].to_string(), style),
            }
        })
        .collect()
}

pub(super) fn mixer_pane(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    if area.height < COMPACT_BELOW_HEIGHT {
        compact_mixer(model, frame, area, hits);
    } else {
        full_mixer(model, frame, area, hits);
    }
}

/// Draw `line` at (`x`, `y`) inside `inner`, `width` cells wide, clipped to
/// the pane.
fn put(frame: &mut Frame, inner: Rect, x: u16, y: u16, width: u16, line: Line<'_>) {
    let rect = Rect::new(x, inner.y + y, width, 1).intersection(inner);
    if !rect.is_empty() {
        frame.render_widget(Paragraph::new(line), rect);
    }
}

fn clipped(inner: Rect, rect: Rect) -> Option<Rect> {
    let rect = rect.intersection(inner);
    (!rect.is_empty()).then_some(rect)
}

fn full_mixer(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let focused = model.pane == Pane::Mixer;
    let snapshot = &model.snapshot;
    frame.render_widget(pane_block("Mixer", focused, model.accent), area);
    let inner = ratatui::widgets::Block::bordered().inner(area);
    // The faders take every row between the label row and the value row.
    let fader_height = inner.height.saturating_sub(2).max(1);
    let value_row = FADER_ROW + fader_height;
    let is_focus = |row: MixerRow| focused && row == model.mixer_row;
    let label_style = |row: MixerRow| {
        locked_style(
            model,
            if is_focus(row) {
                selection_style(model)
            } else {
                Style::new()
            },
        )
    };
    let fader_style = |row: MixerRow| {
        locked_style(
            model,
            if is_focus(row) {
                Style::new().fg(model.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            },
        )
    };

    // Volume.
    let mut x = inner.x + 1;
    put(
        frame,
        inner,
        x,
        0,
        VOLUME_WIDTH,
        Line::styled(" VOL ", label_style(MixerRow::Volume)),
    );
    let filled =
        (f32::from(snapshot.volume.min(100)) / 100.0 * f32::from(fader_height)).round() as u16;
    for row in 0..fader_height {
        let on = fader_height - row <= filled;
        let glyph = if on { "┃" } else { "│" };
        put(
            frame,
            inner,
            x + 2,
            FADER_ROW + row,
            1,
            Line::styled(glyph, fader_style(MixerRow::Volume)),
        );
    }
    put(
        frame,
        inner,
        x,
        value_row,
        VOLUME_WIDTH,
        Line::styled(
            format!("{:^5}", snapshot.volume),
            locked_style(model, Style::new()),
        ),
    );
    if let Some(rect) = clipped(
        inner,
        Rect::new(x, inner.y + FADER_ROW, VOLUME_WIDTH, fader_height),
    ) {
        hits.push(rect, Target::Fader(MixerRow::Volume));
    }
    x += VOLUME_WIDTH + 2;

    // Level meter, sized to what the faders and rates leave over.
    let fixed = 5 * BAND_WIDTH + 3 + 4 + RATE_WIDTH + 2;
    let meter_width = inner
        .width
        .saturating_sub(VOLUME_WIDTH + 2 + fixed)
        .clamp(METER_MIN_WIDTH, METER_MAX_WIDTH);
    put(
        frame,
        inner,
        x,
        0,
        meter_width,
        Line::styled("LVL", locked_style(model, Style::new())),
    );
    let mut cells = meter_cells(&model.meter, usize::from(meter_width));
    if model.locked() || model.core_down {
        for cell in &mut cells {
            cell.style = cell.style.add_modifier(Modifier::DIM);
        }
    }
    put(
        frame,
        inner,
        x,
        FADER_ROW + 1,
        meter_width,
        Line::from(cells),
    );
    x += meter_width + 3;

    // EQ.
    let (lo, hi) = EQ_RANGE_DB;
    for (band, name) in EQ_BANDS.iter().enumerate() {
        let row = MixerRow::Eq(band);
        let gain = snapshot.eq[band];
        put(
            frame,
            inner,
            x,
            0,
            BAND_WIDTH,
            Line::styled(format!("{name:^5}"), label_style(row)),
        );
        let centre = (fader_height - 1) / 2;
        let position = fader_offset(gain, fader_height, lo, hi);
        for line in 0..fader_height {
            let lit = (position.min(centre)..=position.max(centre)).contains(&line);
            let glyph = if line == centre {
                "┼"
            } else if lit {
                "┃"
            } else {
                "│"
            };
            let style = if lit || line == centre && is_focus(row) {
                fader_style(row)
            } else {
                locked_style(model, Style::new())
            };
            put(
                frame,
                inner,
                x + 2,
                FADER_ROW + line,
                1,
                Line::styled(glyph, style),
            );
        }
        if is_focus(row) {
            put(
                frame,
                inner,
                x.saturating_sub(2).max(inner.x),
                value_row,
                DB_LABEL_WIDTH,
                Line::styled(format!("{gain:+.1} dB"), fader_style(row)).centered(),
            );
        }
        if let Some(rect) = clipped(
            inner,
            Rect::new(x, inner.y + FADER_ROW, BAND_WIDTH, fader_height),
        ) {
            hits.push(rect, Target::Fader(row));
        }
        x += BAND_WIDTH;
    }
    x += 4;

    // Sample rates.
    let rates = [(MixerRow::InputRate, "In "), (MixerRow::OutputRate, "Out")];
    for (line, (row, name)) in rates.into_iter().enumerate() {
        let text = match &snapshot.rates {
            Some(rates) => {
                let side = if row == MixerRow::InputRate {
                    &rates.input
                } else {
                    &rates.output
                };
                format!("{name} {:>5} Hz", side.sample_rate_hz)
            }
            None => format!("{name} —"),
        };
        let y = FADER_ROW + line as u16;
        put(
            frame,
            inner,
            x,
            y,
            RATE_WIDTH,
            Line::styled(text, label_style(row)),
        );
        if let Some(rect) = clipped(inner, Rect::new(x, inner.y + y, RATE_WIDTH, 1)) {
            hits.push(rect, Target::Rate(row));
        }
    }
}

/// One line per control, scrolled to the cursor; for windows too short for
/// the faders.
fn compact_mixer(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
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
    for (index, row) in MixerRow::ALL.iter().enumerate() {
        if let Some(rect) = row_rect(area, state.offset(), index) {
            hits.push(
                rect,
                match row {
                    MixerRow::InputRate | MixerRow::OutputRate => Target::Rate(*row),
                    _ => Target::Fader(*row),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_value_endpoints_and_rounding() {
        assert_eq!(fader_value(0, 5, -12.0, 12.0, 0.5), 12.0);
        assert_eq!(fader_value(4, 5, -12.0, 12.0, 0.5), -12.0);
        assert_eq!(fader_value(2, 5, -12.0, 12.0, 0.5), 0.0);
        assert_eq!(fader_value(1, 4, 0.0, 100.0, 1.0), 67.0);
    }

    #[test]
    fn meter_cells_fill_in_eighths_and_mark_the_peak() {
        let text = |meter: &Meter| -> String {
            meter_cells(meter, 10)
                .iter()
                .map(|s| s.content.to_string())
                .collect()
        };
        let silent = Meter::default();
        assert_eq!(text(&silent), " ".repeat(10));
        let half = Meter {
            rms_db: -30.0,
            peak_db: -60.0,
            ..Meter::default()
        };
        assert_eq!(text(&half), "█████     ");
        let eighth = Meter {
            rms_db: -60.0 + 60.0 / 80.0,
            ..Meter::default()
        };
        assert_eq!(text(&eighth), "▏         ");
        let peaked = Meter {
            rms_db: -30.0,
            peak_db: 0.0,
            ..Meter::default()
        };
        assert_eq!(text(&peaked), "█████    │");
    }
}
