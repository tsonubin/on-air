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
use cd::cd_strip;
use header::{hint_line, render_header};
use mixer::mixer_pane;
use overlay::{eject_overlay, help_overlay, pin_overlay, status_overlay, track_overlay};
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

    frame.render_widget(hint_line(), areas.hints);

    match &model.modal {
        Modal::None => {}
        Modal::Help => help_overlay(frame, area),
        Modal::Pin {
            target,
            input,
            error,
        } => pin_overlay(frame, area, target, input, error.as_deref()),
        Modal::TrackPrompt { input, error } => {
            track_overlay(frame, area, input, error.as_deref());
        }
        Modal::ConfirmEject => eject_overlay(frame, area),
        Modal::Status => status_overlay(model, frame, area),
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        ActiveOutput, ActiveOutputView, BluetoothDeviceInfo, CdStatus, OutputInfo, OutputPhase,
        SampleRateResponse, SampleRateSide, StatusResponse,
    };
    use crate::client::Connection;
    use crate::keys::KEYS;
    use crate::model::{Activation, ActivationStep, Modal, Model, PairTarget, Pane, Snapshot};
    use crate::view::hit::CdButton;
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
        render_hits(model, w, h).0
    }

    fn render_hits(model: &Model, w: u16, h: u16) -> (Buffer, HitMap) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        terminal.draw(|frame| hits = view(model, frame)).unwrap();
        (terminal.backend().buffer().clone(), hits)
    }

    /// The cell where `needle` starts, searching below the header line.
    fn cell_of(buffer: &Buffer, needle: &str) -> (u16, u16) {
        let area = buffer.area;
        for y in 1..area.height {
            for x in 0..area.width {
                let rest: String = (x..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect();
                if rest.starts_with(needle) {
                    return (x, y);
                }
            }
        }
        panic!("{needle:?} not rendered:\n{}", text(buffer));
    }

    fn dest_index(model: &Model, id: &str) -> usize {
        model
            .dest_rows()
            .iter()
            .position(|row| matches!(row, crate::model::DestRow::Entry(e) if e.id == id))
            .unwrap()
    }

    fn check_hits(model: &Model, w: u16, h: u16) {
        let (b, hits) = render_hits(model, w, h);
        let (x, y) = cell_of(&b, "Built-in mic");
        assert_eq!(hits.hit(x, y).map(|(t, _)| t), Some(Target::SourceRow(1)));
        let (x, y) = cell_of(&b, "Mock Bluetooth Speaker");
        let want = Target::DestRow(dest_index(model, "bt-speaker"));
        assert_eq!(hits.hit(x, y).map(|(t, _)| t), Some(want));
        let (x, y) = cell_of(&b, "Sonos");
        assert_eq!(hits.hit(x, y).map(|(t, _)| t), None, "headers are inert");
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
        assert!(t.contains("● ON AIR"), "{t}");
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

    fn with_phase(phase: Option<OutputPhase>) -> Model {
        let mut m = model();
        m.snapshot.active_output.as_mut().unwrap().state = phase;
        m
    }

    #[test]
    fn lamp_states() {
        let mut idle = model();
        idle.snapshot.active_output = None;
        let t = text(&render(&idle, 100, 30));
        assert!(t.contains("○ idle"), "{t}");
        let t = text(&render(&with_phase(Some(OutputPhase::Starting)), 100, 30));
        assert!(t.contains("◌ connecting"), "{t}");
        let t = text(&render(&with_phase(Some(OutputPhase::Live)), 100, 30));
        assert!(t.contains("● ON AIR"), "{t}");
        assert!(t.contains("Sonos · Mock Sonos"), "{t}");
        let t = text(&render(&with_phase(Some(OutputPhase::Failed)), 100, 30));
        assert!(t.contains("✗ failed"), "{t}");
        assert!(!t.contains("ON AIR"), "{t}");
    }

    #[test]
    fn header_right_side() {
        let mut m = model();
        m.autostart = Some(true);
        let (b, hits) = render_hits(&m, 100, 30);
        let first = rows(&b)[0].clone();
        for needle in ["PIN 123456", "10.0.0.12", "monitor on", "login ✓"] {
            assert!(first.contains(needle), "{needle} missing: {first}");
        }
        assert!(first.trim_end().ends_with("login ✓"), "{first}");
        let column = |needle: &str| first[..first.find(needle).unwrap()].chars().count() as u16;
        assert_eq!(
            hits.hit(column("monitor on"), 0).map(|(t, _)| t),
            Some(Target::MonitorTag)
        );
        assert_eq!(
            hits.hit(column("PIN"), 0).map(|(t, _)| t),
            Some(Target::StatusTag)
        );
        assert_eq!(
            hits.hit(column("10.0.0.12"), 0).map(|(t, _)| t),
            Some(Target::StatusTag)
        );
    }

    fn tagged_model() -> Model {
        let mut m = model();
        m.snapshot.outputs = vec![
            OutputInfo {
                kind: "pair".into(),
                member_count: 2,
                ..output("uuid:mock-sonos", "Mock Sonos", "sonos", false, true)
            },
            output("ap-pin", "Pin HomePod", "airplay", true, false),
            output("ap-bad", "Broken HomePod", "airplay", false, true),
            output("bt-conn", "Connected Speaker", "bluetooth", false, true),
            output("bt-paired", "Paired Speaker", "bluetooth", false, true),
            output("bt-new", "New Speaker", "bluetooth", true, false),
        ];
        m.snapshot.bluetooth = Some(vec![
            BluetoothDeviceInfo {
                id: "bt-conn".into(),
                name: "Connected Speaker".into(),
                paired: true,
                connected: true,
            },
            BluetoothDeviceInfo {
                id: "bt-paired".into(),
                name: "Paired Speaker".into(),
                paired: true,
                connected: false,
            },
            BluetoothDeviceInfo {
                id: "bt-new".into(),
                name: "New Speaker".into(),
                paired: false,
                connected: false,
            },
        ]);
        m.failed_output = Some(("airplay".into(), "ap-bad".into()));
        m
    }

    fn row_text(b: &Buffer, needle: &str) -> String {
        rows(b)[row_of(b, needle)].clone()
    }

    #[test]
    fn dest_tags() {
        let b = render(&tagged_model(), 100, 30);
        let tag = |name: &str, want: &str| {
            let row = row_text(&b, name);
            assert!(row.contains(want), "{name}: {want:?} missing in {row}");
        };
        tag("Mock Sonos", "⚇ pair");
        tag("Mock Sonos", "live");
        tag("Pin HomePod", "pin");
        tag("Broken HomePod", "✗ failed");
        tag("Connected Speaker", "connected");
        tag("Paired Speaker", "paired");
        tag("New Speaker", "not paired");
        let t = text(&b);
        assert!(!t.contains("not paired  not"), "{t}");

        let mut m = tagged_model();
        m.activation = Some(Activation {
            transport: "airplay".into(),
            id: "ap-pin".into(),
            name: "Pin HomePod".into(),
            step: ActivationStep::Pairing,
        });
        let row = row_text(&render(&m, 100, 30), "Pin HomePod");
        assert!(row.contains("◌ pairing…"), "{row}");
        m.activation.as_mut().unwrap().step = ActivationStep::Connecting;
        let row = row_text(&render(&m, 100, 30), "Pin HomePod");
        assert!(row.contains("◌ connecting…"), "{row}");
        m.snapshot.active_output.as_mut().unwrap().state = Some(OutputPhase::Failed);
        let row = row_text(&render(&m, 100, 30), "Mock Sonos");
        assert!(row.contains("✗ failed"), "{row}");
        assert!(row.contains("⚇ pair"), "{row}");
        // The tag sits at the right edge of the Destination block.
        assert!(row.trim_end().ends_with("✗ failed│"), "{row}");
    }

    #[test]
    fn empty_state_lines() {
        let mut m = model();
        m.snapshot.inputs.clear();
        m.snapshot.active_input = None;
        m.snapshot.outputs.clear();
        m.snapshot.active_output = None;
        m.snapshot.bluetooth = Some(vec![]);
        let t = text(&render(&m, 200, 30));
        for copy in [
            "No capture devices found. Check PipeWire, then press r.",
            "None found on this network — press r to rescan",
            "None found — AirPlay needs OwnTone or pyatv",
            "None paired — press b for Bluetooth settings",
        ] {
            assert!(t.contains(copy), "{copy} missing:\n{t}");
        }
    }

    #[test]
    fn layout_100x30_side_by_side() {
        let m = model();
        let b = render(&m, 100, 30);
        let row = row_of(&b, "Source");
        assert_eq!(row, row_of(&b, "Destination"));
        let line: Vec<char> = rows(&b)[row].chars().collect();
        let second = line.iter().rposition(|c| *c == '┌').unwrap();
        let width = 100 - second;
        assert!((55..=65).contains(&width), "destination width {width}");
        check_hits(&m, 100, 30);
    }

    #[test]
    fn layout_80x24_stacks() {
        let m = model();
        let b = render(&m, 80, 24);
        assert!(row_of(&b, "Source") < row_of(&b, "Destination"));
        assert!(row_of(&b, "Destination") < row_of(&b, "Mixer"));
        check_hits(&m, 80, 24);
    }

    #[test]
    fn the_hint_line_offers_info() {
        let b = render(&model(), 100, 30);
        assert!(rows(&b).last().unwrap().contains("i info"));
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
        assert!(t.contains("New Bluetooth Speaker"), "{t}");
    }

    #[test]
    fn mixer_renders_faders_meter_rates() {
        let mut m = model();
        m.snapshot.volume = 72;
        m.pane = Pane::Mixer;
        m.mixer_row = crate::model::MixerRow::Eq(2);
        let (b, hits) = render_hits(&m, 100, 30);
        let t = text(&b);
        for needle in [
            "VOL", "72", "LVL", "60", "250", "1k", "4k", "12k", "In", "Out", "48000 Hz",
            "44100 Hz", "+2.0 dB",
        ] {
            assert!(t.contains(needle), "{needle} missing:\n{t}");
        }
        let find = |target: Target| {
            (0..30)
                .flat_map(|y| (0..100).map(move |x| (x, y)))
                .find_map(|(x, y)| hits.hit(x, y).filter(|(t, _)| *t == target))
                .map(|(_, rect)| rect)
        };
        for target in [
            Target::Fader(crate::model::MixerRow::Volume),
            Target::Fader(crate::model::MixerRow::Eq(2)),
        ] {
            let rect = find(target).unwrap_or_else(|| panic!("{target:?} missing"));
            assert!(rect.height >= 3, "{target:?} {rect:?}");
        }
        for target in [
            Target::Rate(crate::model::MixerRow::InputRate),
            Target::Rate(crate::model::MixerRow::OutputRate),
        ] {
            assert!(find(target).is_some(), "{target:?} missing");
        }
    }

    #[test]
    fn the_focused_band_label_fits_the_whole_range() {
        let mut m = model();
        m.pane = Pane::Mixer;
        m.mixer_row = crate::model::MixerRow::Eq(4);
        for (gain, label) in [(12.0, "+12.0 dB"), (-12.0, "-12.0 dB"), (-10.5, "-10.5 dB")] {
            m.snapshot.eq[4] = gain;
            let t = text(&render(&m, 100, 30));
            assert!(t.contains(label), "{label} missing:\n{t}");
        }
        m.mixer_row = crate::model::MixerRow::Eq(0);
        m.snapshot.eq[0] = -12.0;
        assert!(text(&render(&m, 100, 30)).contains("-12.0 dB"));
    }

    #[test]
    fn the_focused_band_alone_shows_its_db() {
        let mut m = model();
        m.pane = Pane::Mixer;
        m.mixer_row = crate::model::MixerRow::Eq(2);
        assert!(text(&render(&m, 100, 30)).contains("+2.0 dB"));
        m.mixer_row = crate::model::MixerRow::Volume;
        assert!(!text(&render(&m, 100, 30)).contains("+2.0 dB"));
    }

    #[test]
    fn the_meter_draws_the_rms_bar() {
        let mut m = model();
        m.meter.rms_db = -6.0;
        m.meter.peak_db = -6.0;
        let t = text(&render(&m, 100, 30));
        assert!(t.contains("█"), "{t}");
    }

    #[test]
    fn compact_mixer_below_24_rows() {
        let mut m = model();
        m.pane = Pane::Mixer;
        let t = text(&render(&m, 100, 22));
        assert!(t.contains("Volume") && t.contains("█"), "{t}");
        assert!(!t.contains("VOL"), "{t}");
        m.mixer_row = crate::model::MixerRow::Eq(2);
        let t = text(&render(&m, 100, 22));
        assert!(t.contains("> EQ 1k") && t.contains("+2.0 dB"), "{t}");
    }

    fn playing_disc() -> CdStatus {
        CdStatus {
            present: true,
            playing: true,
            track: 3,
            track_count: 12,
            title: Some("Title".into()),
            album: Some("Album".into()),
            position_ms: 134_000,
            duration_ms: 562_000,
            tracks: vec![],
        }
    }

    #[test]
    fn cd_strip_with_disc() {
        let mut m = model();
        m.snapshot.cd = Some(playing_disc());
        let (b, hits) = render_hits(&m, 100, 30);
        let t = text(&b);
        for needle in ["03/12", "Album", "Title", "02:14 / 09:22", "⏏"] {
            assert!(t.contains(needle), "{needle} missing:\n{t}");
        }
        let seen: Vec<Target> = (0..30)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .filter_map(|(x, y)| hits.hit(x, y).map(|(t, _)| t))
            .collect();
        for target in [
            Target::CdButton(CdButton::Prev),
            Target::CdButton(CdButton::PlayPause),
            Target::CdButton(CdButton::Next),
            Target::CdButton(CdButton::Eject),
            Target::CdProgress,
        ] {
            assert!(seen.contains(&target), "{target:?} missing");
        }
    }

    #[test]
    fn cd_strip_hidden_without_disc() {
        let t = text(&render(&model(), 100, 30));
        assert!(!t.contains("⏏") && !t.contains("00/00"), "{t}");
        let mut m = model();
        let mut cd = playing_disc();
        cd.present = false;
        m.snapshot.cd = Some(cd);
        let t = text(&render(&m, 100, 30));
        assert!(!t.contains("⏏"), "{t}");
    }

    #[test]
    fn cd_overlays_render() {
        let mut m = model();
        m.snapshot.cd = Some(playing_disc());
        m.modal = Modal::ConfirmEject;
        assert!(text(&render(&m, 100, 30)).contains("Eject the disc?"));
        m.modal = Modal::TrackPrompt {
            input: "4".into(),
            error: Some("Track 13 is not on this disc".into()),
        };
        let t = text(&render(&m, 100, 30));
        assert!(
            t.contains("Go to track") && t.contains("not on this disc"),
            "{t}"
        );
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
        assert!(!t.contains("Source"), "{t}");
    }

    #[test]
    fn narrow_terminals_stack_the_panes() {
        let b = render(&model(), 60, 40);
        let sources = row_of(&b, "Source");
        let outputs = row_of(&b, "Destination");
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
    fn status_overlay_lists_everything() {
        let mut m = model();
        m.base = "http://127.0.0.1:47990".into();
        m.autostart = Some(true);
        m.snapshot.status.as_mut().unwrap().lan_addresses =
            Some(vec!["10.0.0.12".into(), "192.168.1.5".into()]);
        m.modal = Modal::Status;
        let t = text(&render(&m, 100, 30));
        for needle in [
            "0.1.0",
            "47990",
            "10.0.0.12:47990",
            "192.168.1.5:47990",
            "owntone",
            "monitor on",
            "sleep inhibited: yes",
            "login start: on",
            "http://127.0.0.1:47990",
        ] {
            assert!(t.contains(needle), "{needle} missing:\n{t}");
        }
    }

    #[test]
    fn monitor_off_panel_replaces_route() {
        let mut m = model();
        m.snapshot.status.as_mut().unwrap().service_enabled = false;
        let (buffer, hits) = render_hits(&m, 100, 30);
        let t = text(&buffer);
        for line in [
            "Monitor is off.",
            "Capture and streaming are stopped. Discovery and the phone remote stay online.",
            "Press m to turn it on.",
        ] {
            assert!(t.contains(line), "{line} missing:\n{t}");
        }
        let narrow = text(&render(&m, 80, 24));
        let flat = narrow.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains(
                "Capture and streaming are stopped. Discovery and the phone remote stay online."
            ),
            "{narrow}"
        );
        assert!(!t.contains("Source"), "{t}");
        assert!(!t.contains("Destination"), "{t}");
        assert!(!t.contains("Mock Sonos "), "{t}");
        for y in 0..30 {
            for x in 0..100 {
                assert!(!matches!(
                    hits.hit(x, y),
                    Some((Target::SourceRow(_) | Target::DestRow(_), _))
                ));
            }
        }
    }

    #[test]
    fn core_down_dims_but_keeps_rows() {
        let mut m = model();
        m.core_down = true;
        let b = render(&m, 100, 30);
        assert!(text(&b).contains("Living Room AirPlay"));
        let (x, y) = cell_of(&b, "Living Room AirPlay");
        assert!(b[(x, y)].modifier.contains(ratatui::style::Modifier::DIM));
        let (x, y) = cell_of(&b, "VOL");
        assert!(b[(x, y)].modifier.contains(ratatui::style::Modifier::DIM));
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
    fn backgrounds_are_reset_everywhere() {
        let b = render(&model(), 100, 30);
        for y in 0..30 {
            for x in 0..100 {
                assert_eq!(b[(x, y)].bg, Color::Reset, "cell ({x}, {y})");
            }
        }
    }
}
