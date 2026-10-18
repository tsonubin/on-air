//! Rendering. Pure over the model: everything on screen is a function of
//! `Model`, so the tests render into a `TestBackend` and assert on text.
//!
//! Backgrounds are `Color::Reset` everywhere so the terminal's own
//! background (and Omarchy's opacity and glass rules) show through;
//! foregrounds are the sixteen ANSI colors plus the theme accent.

mod cd;
mod header;
mod hit;
mod layout;
mod mixer;
mod overlay;
mod route;

pub use hit::{CdButton, HitMap, Target};
pub use layout::truncate;
pub use mixer::fader_value;

use crate::model::{Modal, Model};
use cd::cd_strip;
use header::{hint_line, render_header};
use mixer::mixer_pane;
use overlay::{
    audio_check_overlay, eject_overlay, firewall_overlay, help_overlay, pin_overlay,
    status_overlay, track_overlay, trim_overlay,
};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use route::{dest_pane, monitor_off_panel, source_pane};

/// Below this the screen only asks for a larger window.
pub const MIN_SIZE: (u16, u16) = (40, 10);
/// Below this many columns Source stacks above Destination.
pub const STACK_BELOW_COLUMNS: u16 = 90;
/// Below this many rows the mixer shrinks to its compact form.
pub const COMPACT_BELOW_ROWS: u16 = 24;
/// From this many rows the mixer grows so its faders are five cells tall.
pub const TALL_FADERS_FROM_ROWS: u16 = 30;

pub fn view(model: &Model, frame: &mut Frame) -> HitMap {
    let mut hits = HitMap::default();
    let area = frame.area();
    if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
        let notice = Paragraph::new(format!(
            "on-air needs at least {}×{} cells; please resize the window.",
            MIN_SIZE.0, MIN_SIZE.1
        ))
        .wrap(Wrap { trim: true });
        frame.render_widget(notice, area);
        return hits;
    }
    let areas = layout::areas(model, area);

    render_header(model, frame, areas.header, &mut hits);
    if areas.cd.height == 1 {
        cd_strip(model, frame, areas.cd, &mut hits);
    }
    if let Some(copy) = &model.error {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ✗ ", Style::new().fg(Color::Red)),
                Span::raw(truncate(copy, areas.error.width.saturating_sub(4) as usize)),
            ])),
            areas.error,
        );
    }
    if let Some(copy) = &model.notice {
        if model
            .snapshot
            .status
            .as_ref()
            .is_some_and(|s| s.firewall_warning.is_some())
        {
            hits.push(areas.notice, Target::Firewall);
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ! ", Style::new().fg(Color::Yellow)),
                Span::raw(truncate(
                    copy,
                    areas.notice.width.saturating_sub(4) as usize,
                )),
            ])),
            areas.notice,
        );
    }

    if model.locked() {
        monitor_off_panel(frame, areas.source.union(areas.dest));
    } else {
        source_pane(model, frame, areas.source, &mut hits);
        dest_pane(model, frame, areas.dest, &mut hits);
    }
    mixer_pane(model, frame, areas.mixer, &mut hits);
    if areas.mixer.width >= 32 && areas.mixer.height > 0 {
        let button = ratatui::layout::Rect::new(areas.mixer.right() - 17, areas.mixer.y, 16, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(" [t Audio check] ", Style::new().bold())),
            button,
        );
        hits.push(button, Target::AudioCheck);
    }

    frame.render_widget(hint_line(), areas.hints);

    let overlay = match &model.modal {
        Modal::None => None,
        Modal::Help => Some(help_overlay(frame, area, model.mouse)),
        Modal::Pin {
            target,
            input,
            error,
        } => Some(pin_overlay(
            frame,
            area,
            target,
            input,
            error.as_deref(),
            model.activation.is_some(),
        )),
        Modal::TrackPrompt { input, error } => {
            Some(track_overlay(frame, area, input, error.as_deref()))
        }
        Modal::TrimPrompt {
            transport,
            device_id,
            name,
            input,
            error,
        } => {
            let current = model.sync_row(transport, device_id).map(|row| row.trim_ms);
            Some(trim_overlay(
                frame,
                area,
                name,
                current,
                input,
                error.as_deref(),
                model.pending.is_some(),
            ))
        }
        Modal::ConfirmEject => Some(eject_overlay(frame, area)),
        Modal::Status => Some(status_overlay(model, frame, area)),
        Modal::Firewall => Some(firewall_overlay(model, frame, area)),
        Modal::AudioCheck => Some(audio_check_overlay(model, frame, area)),
    };
    if let Some(rect) = overlay {
        hits.push(area, Target::OverlayOutside);
        hits.push(rect, Target::Overlay);
    }
    hits
}

#[cfg(test)]
mod tests;
