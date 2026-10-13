//! Rendering. Pure over the model: everything on screen is a function of
//! `Model`, so the tests render into a `TestBackend` and assert on text.
//!
//! Backgrounds are `Color::Reset` everywhere so the terminal's own
//! background (and Omarchy's opacity and glass rules) show through;
//! foregrounds are the sixteen ANSI colors plus the theme accent.

use crate::keys::{HINTS, KEYS};
use crate::model::{MixerRow, Modal, Model, OutputRow, Pane};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Below this the screen only asks for a larger window.
pub const MIN_SIZE: (u16, u16) = (40, 10);
/// Below this many columns the three panes stack vertically.
pub const STACK_BELOW_COLUMNS: u16 = 80;

const EQ_BANDS: [&str; 5] = ["60", "250", "1k", "4k", "12k"];
const EQ_CELLS: usize = 9;
const VOLUME_CELLS: usize = 10;

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

pub fn view(model: &Model, frame: &mut Frame) {
    let area = frame.area();
    if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
        let notice = Paragraph::new(format!(
            "on-air needs at least {}×{} cells; please resize the window.",
            MIN_SIZE.0, MIN_SIZE.1
        ))
        .wrap(Wrap { trim: true });
        frame.render_widget(notice, area);
        return;
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
}

fn header_line(model: &Model) -> Paragraph<'_> {
    let mut spans = vec![Span::styled(" on-air ", Style::new().bold())];
    match model.connection {
        crate::client::Connection::Closed | crate::client::Connection::Reconnecting
            if model.snapshot.status.is_none() =>
        {
            spans.push(Span::styled(
                " core unreachable ",
                Style::new().fg(Color::Red),
            ));
        }
        crate::client::Connection::Closed => {
            spans.push(Span::styled(" unreachable ", Style::new().fg(Color::Red)));
        }
        crate::client::Connection::Reconnecting | crate::client::Connection::Connecting => {
            spans.push(Span::styled(
                " reconnecting ",
                Style::new().fg(Color::Yellow),
            ));
        }
        crate::client::Connection::Open => {}
    }
    if !model.service_on() {
        spans.push(Span::styled(
            " service off — press s ",
            Style::new().fg(Color::Yellow).bold(),
        ));
    } else {
        match &model.snapshot.active_output {
            Some(active) if active.is_live() => {
                spans.push(Span::styled(
                    " ● live ",
                    Style::new().fg(Color::Green).bold(),
                ));
                spans.push(Span::raw(format!(
                    "{} · {} ",
                    transport_label(&active.output.transport),
                    active.output.device_name
                )));
            }
            Some(active) => {
                spans.push(Span::styled(" ◌ starting ", Style::new().fg(Color::Yellow)));
                spans.push(Span::raw(format!("{} ", active.output.device_name)));
            }
            None => spans.push(Span::styled(" ○ idle ", Style::new().dim())),
        }
        if let Some(pending) = &model.pending {
            spans.push(Span::styled(
                format!(" {}… ", pending.label),
                Style::new().fg(Color::Yellow),
            ));
        }
        if let Some(pin) = &model.snapshot.pin {
            spans.push(Span::raw(format!(" PIN {pin} ")));
        }
        if let Some(address) = model
            .snapshot
            .status
            .as_ref()
            .and_then(|s| s.lan_addresses.as_ref())
            .and_then(|a| a.first())
        {
            spans.push(Span::styled(format!(" {address} "), Style::new().dim()));
        }
        spans.push(Span::raw(" service on "));
    }
    if let Some(autostart) = model.autostart {
        spans.push(Span::styled(
            if autostart {
                " login ✓ "
            } else {
                " login ✗ "
            },
            Style::new().dim(),
        ));
    }
    Paragraph::new(Line::from(spans))
}

fn transport_label(transport: &str) -> &'static str {
    crate::model::TRANSPORTS
        .iter()
        .find(|(id, _)| *id == transport)
        .map(|(_, label)| *label)
        .unwrap_or("Output")
}

fn cd_line(model: &Model) -> Paragraph<'_> {
    let Some(cd) = &model.snapshot.cd else {
        return Paragraph::new("");
    };
    let glyph = if cd.playing { "▶" } else { "⏸" };
    let title = cd
        .title
        .clone()
        .or_else(|| cd.album.clone())
        .unwrap_or_default();
    Paragraph::new(Line::from(vec![
        Span::styled(format!(" {glyph} Audio CD "), Style::new().fg(Color::Cyan)),
        Span::raw(format!(" {:02}/{:02}  ", cd.track, cd.track_count)),
        Span::raw(title),
        Span::styled(
            format!("  {}/{}", clock(cd.position_ms), clock(cd.duration_ms)),
            Style::new().dim(),
        ),
    ]))
}

fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn pane_block<'a>(title: &'a str, focused: bool, accent: Color) -> Block<'a> {
    let block = Block::bordered().title(format!(" {title} "));
    if focused {
        block.border_style(Style::new().fg(accent))
    } else {
        block
    }
}

fn selection_style(model: &Model) -> Style {
    Style::new()
        .fg(model.accent)
        .add_modifier(Modifier::REVERSED)
}

fn locked_style(model: &Model, style: Style) -> Style {
    if model.locked() {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

fn sources_pane(model: &Model, frame: &mut Frame, area: Rect) {
    let focused = model.pane == Pane::Sources;
    let block = pane_block("Sources", focused, model.accent);
    let inner_width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = model
        .snapshot
        .inputs
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let selected = focused && index == model.source_index;
            let active = model.snapshot.active_input.as_deref() == Some(name.as_str());
            let marker = if selected { "> " } else { "  " };
            let label = truncate(name, inner_width.saturating_sub(4));
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

fn outputs_pane(model: &Model, frame: &mut Frame, area: Rect) {
    let focused = model.pane == Pane::Outputs;
    let block = pane_block("Outputs", focused, model.accent);
    let inner_width = area.width.saturating_sub(4) as usize;
    let live_id = model
        .snapshot
        .active_output
        .as_ref()
        .map(|a| (a.output.transport.as_str(), a.output.device_id.as_str()));
    let items: Vec<ListItem> = model
        .output_rows()
        .into_iter()
        .enumerate()
        .map(|(row_index, row)| match row {
            OutputRow::Header(label) => {
                ListItem::new(Line::from(Span::styled(label, Style::new().bold())))
                    .style(locked_style(model, Style::new()))
            }
            OutputRow::Device(index) => {
                let output = &model.snapshot.outputs[index];
                let selected = focused && row_index == model.output_index;
                let marker = if selected { "> " } else { "  " };
                let suffix = if output.needs_pair && !output.paired {
                    " (pair)"
                } else {
                    ""
                };
                let is_live = live_id == Some((output.transport.as_str(), output.id.as_str()));
                let budget =
                    inner_width.saturating_sub(2 + suffix.len() + if is_live { 7 } else { 0 });
                let mut spans = vec![
                    Span::raw(marker),
                    Span::raw(format!("{}{suffix}", truncate(&output.name, budget))),
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

fn bar(filled: usize, cells: usize) -> String {
    let filled = filled.min(cells);
    format!("{}{}", "█".repeat(filled), "░".repeat(cells - filled))
}

fn eq_band(gain_db: f32) -> String {
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

fn mixer_pane(model: &Model, frame: &mut Frame, area: Rect) {
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

fn hint_line() -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (key, what) in HINTS {
        spans.push(Span::styled(format!(" {key} "), Style::new().bold()));
        spans.push(Span::styled(format!("{what} "), Style::new().dim()));
    }
    Paragraph::new(Line::from(spans))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(vertical);
    horizontal
}

fn help_overlay(frame: &mut Frame, area: Rect) {
    let width = area.width.min(80);
    let height = (KEYS.len() as u16 + 3).min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|binding| {
            Line::from(vec![
                Span::styled(format!(" {:<16} ", binding.keys), Style::new().bold()),
                Span::raw(binding.help),
            ])
        })
        .chain(std::iter::once(Line::from(Span::styled(
            " any key closes ",
            Style::new().dim(),
        ))))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Keys ")),
        rect,
    );
}

fn pin_overlay(
    frame: &mut Frame,
    area: Rect,
    target: &crate::model::PairTarget,
    input: &str,
    error: Option<&str>,
) {
    let width = area.width.min(60);
    let rect = centered(area, width, 7);
    frame.render_widget(Clear, rect);
    let prompt = if target.needs_pin() {
        "Enter the PIN shown by the speaker, then press Enter."
    } else {
        "Press Enter to pair, Esc to cancel."
    };
    let shown = if target.needs_pin() {
        format!("PIN: {input}▏")
    } else {
        String::new()
    };
    let mut lines = vec![
        Line::from(prompt),
        Line::from(Span::styled(shown, Style::new().bold())),
    ];
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Pair {} ", target.name()))),
        rect,
    );
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
        };
        model
    }

    fn render(model: &Model, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|frame| view(model, frame)).unwrap();
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
    fn the_header_shows_live_output_pin_and_service() {
        let t = text(&render(&model(), 100, 30));
        assert!(t.contains("● live"), "{t}");
        assert!(t.contains("Mock Sonos"), "{t}");
        assert!(t.contains("PIN 123456"), "{t}");
        assert!(t.contains("10.0.0.12"), "{t}");
        assert!(t.contains("service on"), "{t}");
    }

    #[test]
    fn the_header_says_service_off() {
        let mut m = model();
        m.snapshot.status.as_mut().unwrap().service_enabled = false;
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("service off — press s"), "{t}");
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
        m.pane = Pane::Outputs;
        m.output_index = m.output_rows().len() - 1;
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
