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

use crate::model::{Modal, Model};
use cd::cd_line;
use header::{header_line, hint_line};
use mixer::mixer_pane;
use overlay::{help_overlay, pin_overlay};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use route::{outputs_pane, sources_pane};

/// Below this the screen only asks for a larger window.
pub const MIN_SIZE: (u16, u16) = (40, 10);
/// Below this many columns the three panes stack vertically.
pub const STACK_BELOW_COLUMNS: u16 = 80;

pub fn view(model: &Model, frame: &mut Frame) -> HitMap {
    let hits = HitMap::default();
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
    let cd_rows = u16::from(model.snapshot.cd.as_ref().is_some_and(|cd| cd.present));
    let error_rows = u16::from(model.error.is_some());
    let notice_rows = u16::from(model.notice.is_some());
    let [header, cd, error, notice, panes, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(cd_rows),
        Constraint::Length(error_rows),
        Constraint::Length(notice_rows),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(header_line(model), header);
    if cd_rows == 1 {
        frame.render_widget(cd_line(model), cd);
    }
    if let Some(copy) = &model.error {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ✗ ", Style::new().fg(Color::Red)),
                Span::raw(truncate(copy, error.width.saturating_sub(4) as usize)),
            ])),
            error,
        );
    }
    if let Some(copy) = &model.notice {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ! ", Style::new().fg(Color::Yellow)),
                Span::raw(truncate(copy, notice.width.saturating_sub(4) as usize)),
            ])),
            notice,
        );
    }

    let pane_areas: [Rect; 3] = if area.width < STACK_BELOW_COLUMNS {
        Layout::vertical([
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(35),
        ])
        .areas(panes)
    } else {
        Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .areas(panes)
    };
    sources_pane(model, frame, pane_areas[0]);
    outputs_pane(model, frame, pane_areas[1]);
    mixer_pane(model, frame, pane_areas[2]);

    frame.render_widget(hint_line(), hints);

    match &model.modal {
        Modal::None => {}
        Modal::Help => help_overlay(frame, area),
        Modal::Pin {
            target,
            input,
            error,
        } => pin_overlay(frame, area, target, input, error.as_deref()),
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        ActiveOutput, ActiveOutputView, CdStatus, OutputInfo, OutputPhase, SampleRateResponse,
        SampleRateSide, StatusResponse,
    };
    use crate::client::Connection;
    use crate::keys::KEYS;
    use crate::model::{Modal, Model, PairTarget, Pane, Snapshot};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn output(id: &str, name: &str, transport: &str, needs_pair: bool, paired: bool) -> OutputInfo {
        OutputInfo {
            id: id.into(),
            name: name.into(),
            transport: transport.into(),
            kind: "solo".into(),
            member_count: 1,
            needs_pair,
            paired,
        }
    }

    fn model() -> Model {
        let mut model = Model::new();
        model.connection = Connection::Open;
        model.snapshot = Snapshot {
            status: Some(StatusResponse {
                status: "ok".into(),
                version: "0.1.0".into(),
                service_enabled: true,
                sleep_inhibited: Some(true),
                lan_addresses: Some(vec!["10.0.0.12".into()]),
            }),
            pin: Some("123456".into()),
            inputs: vec!["Mock Monitor".into(), "Built-in mic".into()],
            active_input: Some("Mock Monitor".into()),
            outputs: vec![
                output("uuid:mock-sonos", "Mock Sonos", "sonos", false, true),
                output("ap-living", "Living Room AirPlay", "airplay", false, true),
                output(
                    "bt-speaker",
                    "Mock Bluetooth Speaker",
                    "bluetooth",
                    false,
                    true,
                ),
                output(
                    "bt-unpaired",
                    "New Bluetooth Speaker",
                    "bluetooth",
                    true,
                    false,
                ),
            ],
            active_output: Some(ActiveOutputView {
                output: ActiveOutput {
                    transport: "sonos".into(),
                    device_id: "uuid:mock-sonos".into(),
                    device_name: "Mock Sonos".into(),
                },
                state: Some(OutputPhase::Live),
            }),
            volume: 80,
            eq: [0.0, 0.0, 2.0, 0.0, 0.0],
            rates: Some(SampleRateResponse {
                sample_rate_hz: 48000,
                input: SampleRateSide {
                    sample_rate_hz: 48000,
                    supported_hz: vec![44100, 48000],
                    transport: None,
                },
                output: SampleRateSide {
                    sample_rate_hz: 44100,
                    supported_hz: vec![44100, 48000, 96000],
                    transport: Some("sonos".into()),
                },
            }),
            cd: None,
            airplay_mode: Some("owntone".into()),
            bluetooth: None,
        };
        model
    }

    fn render(model: &Model, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| {
                let _ = view(model, frame);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn rows(buffer: &Buffer) -> Vec<String> {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn text(buffer: &Buffer) -> String {
        rows(buffer).join("\n")
    }

    /// First row below the header line that contains `needle`.
    fn row_of(buffer: &Buffer, needle: &str) -> usize {
        rows(buffer)
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, row)| row.contains(needle))
            .map(|(index, _)| index)
            .unwrap_or_else(|| panic!("{needle:?} not rendered:\n{}", text(buffer)))
    }

    #[test]
    fn the_header_shows_live_output_pin_and_monitor() {
        let t = text(&render(&model(), 100, 30));
        assert!(t.contains("● live"), "{t}");
        assert!(t.contains("Mock Sonos"), "{t}");
        assert!(t.contains("PIN 123456"), "{t}");
        assert!(t.contains("10.0.0.12"), "{t}");
        assert!(t.contains("monitor on"), "{t}");
    }

    #[test]
    fn the_header_says_monitor_off() {
        let mut m = model();
        m.snapshot.status.as_mut().unwrap().service_enabled = false;
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("monitor off — press m"), "{t}");
    }

    #[test]
    fn the_notice_line_sits_under_the_header_and_survives_a_long_header() {
        let mut m = model();
        m.snapshot
            .active_output
            .as_mut()
            .unwrap()
            .output
            .device_name = "X".repeat(80);
        m.notice = Some("sleep not inhibited — the laptop may suspend".into());
        let b = render(&m, 100, 30);
        assert_eq!(row_of(&b, "sleep not inhibited"), 1, "{}", text(&b));
        let t = text(&render(&model(), 100, 30));
        assert!(!t.contains("sleep not inhibited"), "{t}");
    }

    #[test]
    fn the_header_shows_unreachable() {
        let mut m = model();
        m.connection = Connection::Closed;
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("unreachable"), "{t}");
    }

    #[test]
    fn the_focused_row_is_marked() {
        let t = text(&render(&model(), 100, 30));
        assert!(t.contains("> Mock Monitor"), "{t}");
        assert!(t.contains("  Built-in mic"), "{t}");
    }

    #[test]
    fn sources_show_friendly_labels() {
        let mut m = model();
        m.snapshot.inputs = vec!["alsa_output.usb.monitor".into(), "Audio CD".into()];
        m.snapshot.active_input = Some("alsa_output.usb.monitor".into());
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("System monitor ●"), "{t}");
        assert!(t.contains("Audio CD"), "{t}");
        assert!(!t.contains("alsa_output"), "{t}");
    }

    #[test]
    fn outputs_are_grouped_under_transport_headers() {
        let b = render(&model(), 100, 30);
        assert!(row_of(&b, "Sonos") < row_of(&b, "Mock Sonos"));
        assert!(row_of(&b, "AirPlay") < row_of(&b, "Living Room AirPlay"));
        assert!(row_of(&b, "Bluetooth") < row_of(&b, "Mock Bluetooth Speaker"));
        let t = text(&b);
        assert!(t.contains("New Bluetooth Speaker (pair)"), "{t}");
    }

    #[test]
    fn the_mixer_shows_volume_eq_and_rates() {
        let t = text(&render(&model(), 100, 30));
        for needle in [
            "Volume",
            "80",
            "EQ 60",
            "EQ 12k",
            "+2.0 dB",
            "In 48000",
            "Out 44100",
        ] {
            assert!(t.contains(needle), "{needle} missing:\n{t}");
        }
    }

    #[test]
    fn the_cd_row_appears_only_with_a_disc() {
        let t = text(&render(&model(), 100, 30));
        assert!(!t.contains("Audio CD"), "{t}");
        let mut m = model();
        m.snapshot.cd = Some(CdStatus {
            present: true,
            playing: true,
            track: 3,
            track_count: 12,
            title: Some("Track title".into()),
            album: None,
            position_ms: 134_000,
            duration_ms: 270_000,
            tracks: vec![],
        });
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("Audio CD"), "{t}");
        assert!(t.contains("03/12"), "{t}");
        assert!(t.contains("02:14/04:30"), "{t}");
    }

    #[test]
    fn the_hint_line_lists_the_key_table() {
        let b = render(&model(), 100, 30);
        let last = rows(&b).last().unwrap().clone();
        assert!(last.contains("? help"), "{last}");
        assert!(last.contains("q quit"), "{last}");
    }

    #[test]
    fn below_min_size_only_the_resize_notice_renders() {
        let t = text(&render(&model(), 30, 5));
        assert!(t.contains("resize"), "{t}");
        assert!(!t.contains("Sources"), "{t}");
    }

    #[test]
    fn narrow_terminals_stack_the_panes() {
        let b = render(&model(), 60, 40);
        let sources = row_of(&b, "Sources");
        let outputs = row_of(&b, "Outputs");
        let mixer = row_of(&b, "Mixer");
        assert!(sources < outputs && outputs < mixer, "{}", text(&b));
        for row in [sources, outputs, mixer] {
            assert_eq!(rows(&b)[row].find('┌'), Some(0), "{}", text(&b));
        }
    }

    #[test]
    fn long_names_are_truncated_to_the_pane() {
        let mut m = model();
        let long = "A".repeat(60);
        m.snapshot.outputs[0].name = long.clone();
        let b = render(&m, 100, 30);
        let t = text(&b);
        assert!(!t.contains(&long), "{t}");
        assert!(t.contains("…"), "{t}");
        for row in rows(&b) {
            assert!(row.chars().count() <= 100);
        }
        assert_eq!(truncate("héllo wörld", 5), "héll…");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn the_pin_modal_and_help_overlay_render_centered() {
        let mut m = model();
        m.modal = Modal::Pin {
            target: PairTarget::Airplay {
                device_id: "ap-new".into(),
                name: "HomePod".into(),
            },
            input: "12".into(),
            error: Some("That PIN did not match.".into()),
        };
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("Pair HomePod"), "{t}");
        assert!(t.contains("12"), "{t}");
        assert!(t.contains("That PIN did not match."), "{t}");

        let mut m = model();
        m.modal = Modal::Help;
        let t = text(&render(&m, 100, 40));
        for binding in KEYS {
            assert!(t.contains(binding.help), "{} missing:\n{t}", binding.help);
        }
    }

    #[test]
    fn the_outputs_pane_scrolls_to_keep_the_selection_visible() {
        let mut m = model();
        for i in 0..30 {
            m.snapshot.outputs.push(output(
                &format!("bt-{i}"),
                &format!("Speaker {i:02}"),
                "bluetooth",
                false,
                true,
            ));
        }
        m.snapshot.bluetooth = Some(vec![]);
        m.pane = Pane::Outputs;
        m.output_index = m.dest_rows().len() - 1;
        let t = text(&render(&m, 100, 20));
        assert!(t.contains("> Speaker 29"), "{t}");
    }

    #[test]
    fn the_mixer_scrolls_in_a_short_stacked_window() {
        let mut m = model();
        m.pane = Pane::Mixer;
        m.mixer_row = crate::model::MixerRow::OutputRate;
        let t = text(&render(&m, 60, 24));
        assert!(t.contains("> Out 44100"), "{t}");
    }

    #[test]
    fn backgrounds_are_reset_everywhere() {
        let b = render(&model(), 100, 30);
        for y in 0..30 {
            for x in 0..100 {
                assert_eq!(b[(x, y)].bg, Color::Reset, "cell ({x}, {y})");
            }
        }
    }
}
