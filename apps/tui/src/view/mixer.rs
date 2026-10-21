//! The Mixer pane: percentage volume, signal metering, EQ and audio format.
//! Narrow or short windows use a scrolling list with the same controls.

use super::hit::{HitMap, Target};
use super::layout::{locked_style, pane_block, selection_style, truncate};
use crate::model::METER_FLOOR_DB;
use crate::model::{Meter, MixerRow, Model, Pane};
use crate::update::EQ_RANGE_DB;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

const EQ_BANDS: [&str; 5] = ["60", "250", "1k", "4k", "12k"];
const EQ_CELLS: usize = 9;
/// Pane height (with borders) below which the compact form is used.
const COMPACT_BELOW_HEIGHT: u16 = 7;
const FADER_ROW: u16 = 1;
const FULL_MIN_WIDTH: u16 = 78;
const RATE_WIDTH: u16 = 18;
const EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Eighth-cell resolution keeps the displayed percentage and fill aligned.
fn volume_bar(volume: u8, cells: usize) -> String {
    let filled = usize::from(volume.min(100)) * cells * 8 / 100;
    (0..cells)
        .map(|cell| match filled.saturating_sub(cell * 8).min(8) {
            0 => '─',
            n => EIGHTHS[n - 1],
        })
        .collect()
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
    if area.height < COMPACT_BELOW_HEIGHT || area.width < FULL_MIN_WIDTH {
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

fn control_style(model: &Model, row: MixerRow) -> Style {
    locked_style(
        model,
        if model.pane == Pane::Mixer && model.mixer_row == row {
            selection_style(model)
        } else {
            Style::new()
        },
    )
}

fn full_mixer(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    frame.render_widget(
        pane_block("Mixer", model.pane == Pane::Mixer, model.accent),
        area,
    );
    let inner = ratatui::widgets::Block::bordered().inner(area);
    let volume_width = (inner.width * 3 / 10).clamp(22, 32);
    let [volume, first_divider, eq, second_divider, format] = Layout::horizontal([
        Constraint::Length(volume_width),
        Constraint::Length(1),
        Constraint::Min(30),
        Constraint::Length(1),
        Constraint::Length(RATE_WIDTH),
    ])
    .areas(inner);
    for divider in [first_divider, second_divider] {
        for y in 0..divider.height {
            put(
                frame,
                divider,
                divider.x,
                y,
                1,
                Line::styled("│", Style::new().dim()),
            );
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(" EQ · dB ").centered()),
        Rect::new(eq.x, area.y, eq.width, 1),
    );
    volume_section(model, frame, volume, hits);
    eq_section(model, frame, eq, hits);
    format_section(model, frame, format, hits);
}

/// Volume is a percentage control. Signal is measured before output volume;
/// keeping them apart avoids presenting a moving level meter as loudness.
fn volume_section(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let x = area.x + 1;
    let width = area.width.saturating_sub(2);
    put(
        frame,
        area,
        x,
        0,
        width,
        Line::styled(
            format!("Volume {:>3}%", model.snapshot.volume),
            control_style(model, MixerRow::Volume),
        ),
    );
    let cells: Vec<_> = volume_bar(model.snapshot.volume, width as usize)
        .chars()
        .map(|cell| {
            let style = if cell == '─' {
                Style::new().dim()
            } else {
                Style::new().fg(model.accent)
            };
            Span::styled(cell.to_string(), locked_style(model, style))
        })
        .collect();
    put(frame, area, x, 1, width, Line::from(cells));
    hits.push(Rect::new(x, area.y + 1, width, 1), Target::VolumeSlider);
    if area.height >= 7 {
        let output = if model.core_down {
            "Core unavailable"
        } else if !model.service_on() {
            "Monitor off"
        } else {
            model
                .snapshot
                .active_output
                .as_ref()
                .map(|active| active.output.device_name.as_str())
                .unwrap_or("No output")
        };
        put(
            frame,
            area,
            x,
            2,
            width,
            Line::styled(
                truncate(output, width as usize),
                locked_style(model, Style::new().dim()),
            ),
        );
    }
    let signal_row = area.height.saturating_sub(3);
    put(
        frame,
        area,
        x,
        signal_row,
        width,
        Line::styled(
            format!("Signal {:>5.1} dBFS", model.meter.rms_db),
            locked_style(model, Style::new()),
        ),
    );
    let mut cells = meter_cells(&model.meter, width as usize);
    for cell in &mut cells {
        cell.style = locked_style(model, cell.style);
    }
    put(frame, area, x, signal_row + 1, width, Line::from(cells));
    put(
        frame,
        area,
        x,
        signal_row + 2,
        width,
        Line::styled(
            format!("Peak   {:>5.1} dBFS", model.meter.peak_db),
            locked_style(model, Style::new().fg(meter_color(model.meter.peak_db))),
        ),
    );
}

fn eq_section(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let band_width = area.width / EQ_BANDS.len() as u16;
    let start = area.x + (area.width - band_width * EQ_BANDS.len() as u16) / 2;
    let height = area.height.saturating_sub(2).max(1);
    let (lo, hi) = EQ_RANGE_DB;
    for (band, name) in EQ_BANDS.iter().enumerate() {
        let row = MixerRow::Eq(band);
        let x = start + band as u16 * band_width;
        let focused = model.pane == Pane::Mixer && model.mixer_row == row;
        let style = locked_style(
            model,
            if focused {
                Style::new().fg(model.accent).bold()
            } else {
                Style::new()
            },
        );
        let gain = model.snapshot.eq[band];
        put(
            frame,
            area,
            x,
            0,
            band_width,
            Line::styled(*name, control_style(model, row)).centered(),
        );
        let centre = fader_offset(0.0, height, lo, hi);
        let position = fader_offset(gain, height, lo, hi);
        for offset in 0..height {
            let lit = (position.min(centre)..=position.max(centre)).contains(&offset);
            let glyph = if offset == position {
                "●"
            } else if offset == centre {
                "┼"
            } else if lit {
                "┃"
            } else {
                "│"
            };
            put(
                frame,
                area,
                x + band_width / 2,
                FADER_ROW + offset,
                1,
                Line::styled(glyph, style),
            );
        }
        put(
            frame,
            area,
            x,
            area.height - 1,
            band_width,
            Line::styled(format!("{gain:+.1}"), control_style(model, row)).centered(),
        );
        if let Some(rect) = clipped(area, Rect::new(x, area.y + FADER_ROW, band_width, height)) {
            hits.push(rect, Target::Fader(row));
        }
    }
}

fn format_section(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let x = area.x + 1;
    let width = area.width.saturating_sub(1);
    put(
        frame,
        area,
        x,
        0,
        width,
        Line::styled("Audio format", locked_style(model, Style::new().bold())),
    );
    for (index, (row, label)) in [(MixerRow::InputRate, "In "), (MixerRow::OutputRate, "Out")]
        .into_iter()
        .enumerate()
    {
        let text = match &model.snapshot.rates {
            Some(rates) => {
                let side = if row == MixerRow::InputRate {
                    &rates.input
                } else {
                    &rates.output
                };
                format!("{label} {:>5} Hz", side.sample_rate_hz)
            }
            None => format!("{label} —"),
        };
        let y = 1 + index as u16;
        put(
            frame,
            area,
            x,
            y,
            width,
            Line::styled(text, control_style(model, row)),
        );
        hits.push(Rect::new(x, area.y + y, width, 1), Target::Rate(row));
    }
    let buffer = match model
        .snapshot
        .status
        .as_ref()
        .and_then(|status| status.capture_buffer_ms)
    {
        Some(0) => "Buffer default".into(),
        Some(ms) => format!("Buffer {ms} ms"),
        None => "Buffer —".into(),
    };
    let y = area.height.saturating_sub(1);
    put(
        frame,
        area,
        x,
        y,
        width,
        Line::styled(buffer, locked_style(model, Style::new())),
    );
    hits.push(Rect::new(x, area.y + y, width, 1), Target::AudioCheck);
}

/// Keep volume visible even while the remaining controls scroll to focus.
fn compact_mixer(model: &Model, frame: &mut Frame, area: Rect, hits: &mut HitMap) {
    let focused = model.pane == Pane::Mixer;
    let buffer = model
        .snapshot
        .status
        .as_ref()
        .and_then(|status| status.capture_buffer_ms)
        .map(|ms| {
            if ms == 0 {
                "default buffer".into()
            } else {
                format!("{ms} ms buffer")
            }
        });
    let title = buffer
        .map(|buffer| format!("Mixer · {buffer}"))
        .unwrap_or_else(|| "Mixer".into());
    let block = pane_block(&title, focused, model.accent);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let volume_cells = usize::from(inner.width.saturating_sub(18)).clamp(8, 24);
    let snapshot = &model.snapshot;
    let marker = if focused && model.mixer_row == MixerRow::Volume {
        "> "
    } else {
        "  "
    };
    let volume = Line::styled(
        format!(
            "{marker}Volume   {}  {:>3}%",
            volume_bar(snapshot.volume, volume_cells),
            snapshot.volume
        ),
        control_style(model, MixerRow::Volume),
    );
    let volume_rect = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(Paragraph::new(volume), volume_rect);
    hits.push(volume_rect, Target::Fader(MixerRow::Volume));
    let bar = Rect::new(inner.x + 11, inner.y, volume_cells as u16, 1).intersection(volume_rect);
    if !bar.is_empty() {
        hits.push(bar, Target::VolumeSlider);
    }

    let controls = &MixerRow::ALL[1..];
    let content = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    let rows = controls.iter().map(|row| {
        let text = match row {
            MixerRow::Volume => unreachable!("volume has its own fixed row"),
            MixerRow::Eq(band) => format!(
                "EQ {:<4} {}  {:+.1} dB",
                EQ_BANDS[*band],
                eq_band(snapshot.eq[*band]),
                snapshot.eq[*band]
            ),
            MixerRow::InputRate => snapshot
                .rates
                .as_ref()
                .map(|rates| format!("In {} Hz", rates.input.sample_rate_hz))
                .unwrap_or_else(|| "In —".into()),
            MixerRow::OutputRate => snapshot
                .rates
                .as_ref()
                .map(|rates| format!("Out {} Hz", rates.output.sample_rate_hz))
                .unwrap_or_else(|| "Out —".into()),
        };
        let selected = focused && *row == model.mixer_row;
        let marker = if selected { "> " } else { "  " };
        ListItem::new(Line::from(format!("{marker}{text}"))).style(control_style(model, *row))
    });
    let selected = controls.iter().position(|row| *row == model.mixer_row);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(List::new(rows), content, &mut state);
    for (index, row) in controls.iter().enumerate().skip(state.offset()) {
        let y = content.y + (index - state.offset()) as u16;
        if y >= content.bottom() {
            break;
        }
        hits.push(
            Rect::new(content.x, y, content.width, 1),
            match row {
                MixerRow::InputRate | MixerRow::OutputRate => Target::Rate(*row),
                _ => Target::Fader(*row),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_fill_has_precise_endpoints_and_distinguishes_near_maximum() {
        assert_eq!(volume_bar(0, 20), "─".repeat(20));
        assert_eq!(
            volume_bar(80, 20),
            format!("{}{}", "█".repeat(16), "─".repeat(4))
        );
        assert_eq!(volume_bar(100, 20), "█".repeat(20));
        assert_ne!(volume_bar(99, 20), volume_bar(100, 20));
    }

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
