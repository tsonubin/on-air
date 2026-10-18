//! The help, pairing and CD overlays.

use super::layout::centered;
use crate::keys::KEYS;
use crate::model::{Modal, Model};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// How many rows one logical line takes once the overlay's `Paragraph`
/// wraps it at `inner` cells: words break on spaces and a word longer
/// than the line wraps inside itself.
fn line_rows(line: &Line, inner: usize) -> usize {
    let inner = inner.max(1);
    let mut rows = 1;
    let mut used = 0;
    let text: String = line.iter().map(|span| span.content.as_ref()).collect();
    for word in text.split(' ') {
        let width = word.width();
        if width == 0 {
            continue;
        }
        if used > 0 && used + 1 + width <= inner {
            used += 1 + width;
            continue;
        }
        if used > 0 {
            rows += 1;
        }
        // A fresh row for this word; long words span rows of their own.
        let spanned = width.div_ceil(inner);
        rows += spanned - 1;
        used = width - (spanned - 1) * inner;
    }
    rows
}

/// The height of a wrapped overlay box for `lines` at `width` cells: the
/// rows the copy really takes, two borders and two spare rows. Counting
/// logical lines instead would clip the controls at the bottom whenever
/// a sentence wraps.
fn overlay_height(lines: &[Line<'_>], width: u16, area: Rect) -> u16 {
    let inner = usize::from(width.saturating_sub(2));
    let rows: usize = lines.iter().map(|line| line_rows(line, inner)).sum();
    u16::try_from(rows + 4).unwrap_or(u16::MAX).min(area.height)
}

/// The key table, then the mouse rows. The table has more rows than a
/// 30-row terminal can show, so the content scrolls; `scroll` is the
/// number of rows pushed off the top.
pub(super) fn help_overlay(frame: &mut Frame, area: Rect, mouse: bool, scroll: u16) -> Rect {
    let row = |keys: &str, help: Span<'static>| {
        Line::from(vec![
            Span::styled(format!(" {keys:<16} "), Style::new().bold()),
            help,
        ])
    };
    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|binding| row(binding.keys, Span::raw(binding.help)))
        .collect();
    lines.push(row(
        "Mouse",
        if mouse {
            Span::raw("Click rows, buttons and tags; drag or wheel faders")
        } else {
            Span::styled("mouse off", Style::new().dim())
        },
    ));
    if mouse {
        lines.push(row(
            "",
            Span::raw("Hold Shift while dragging to select text."),
        ));
    }
    lines.push(Line::from(Span::styled(
        " any key or a click outside closes ",
        Style::new().dim(),
    )));
    let width = area.width.min(80);
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .block(Block::bordered().title(" Keys ")),
        rect,
    );
    rect
}

pub(super) fn pin_overlay(
    frame: &mut Frame,
    area: Rect,
    target: &crate::model::PairTarget,
    input: &str,
    error: Option<&str>,
    pairing: bool,
) -> Rect {
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
    if pairing {
        lines.push(Line::from(Span::styled(
            "Pairing…",
            Style::new().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Pair {} ", target.name()))),
        rect,
    );
    rect
}

pub(super) fn track_overlay(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    error: Option<&str>,
) -> Rect {
    let rect = centered(area, area.width.min(44), 6);
    frame.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from("Type a track number, then press Enter."),
        Line::from(Span::styled(
            format!("Track: {input}▏"),
            Style::new().bold(),
        )),
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
            .block(Block::bordered().title(" Go to track ")),
        rect,
    );
    rect
}

pub(super) fn eject_overlay(frame: &mut Frame, area: Rect) -> Rect {
    let rect = centered(area, area.width.min(36), 5);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Eject the disc?"),
            Line::from(Span::styled("y eject   n cancel", Style::new().dim())),
        ])
        .block(Block::bordered().title(" Eject ")),
        rect,
    );
    rect
}

/// The core's default port, for a base URL that does not parse.
const DEFAULT_PORT: u16 = 47990;

pub(super) fn audio_check_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let current = model
        .snapshot
        .status
        .as_ref()
        .and_then(|s| s.capture_buffer_ms)
        .unwrap_or(0);
    let label = |ms| {
        if ms == 0 {
            "system default".to_string()
        } else {
            format!("{ms} ms")
        }
    };
    let mut lines = vec![
        Line::from(format!("Current requested buffer: {}", label(current))),
        Line::from(format!(
            "Selected buffer: {}   ← / → to choose",
            label(model.buffer_choice_ms)
        )),
    ];
    match model.tuning.as_ref() {
        None => lines.push(Line::from("First-use tuning: status not loaded.")),
        Some(tuning) => {
            let recommend = |quality: Option<crate::api::DspQuality>| {
                quality
                    .map(|q| crate::cli::quality_label(q).to_string())
                    .unwrap_or_else(|| "unmeasured".into())
            };
            match &tuning.automatic {
                Some(record) => {
                    let applied = match (record.applied_buffer, record.applied_quality) {
                        (true, true) => "both settings were set",
                        (true, false) => "the capture buffer was set; your quality choice was kept",
                        (false, true) => "the DSP quality was set; your buffer choice was kept",
                        (false, false) => "your own settings were kept",
                    };
                    lines.push(Line::from(format!(
                        "First-use tuning: this machine was measured and recommends {} ms buffer, \
                         {} quality; {applied}.",
                        record.recommended_buffer_ms,
                        recommend(tuning.recommended_quality),
                    )));
                }
                None if tuning.tuned => lines.push(Line::from(format!(
                    "An audio check ran here before (recommends {} quality); settings are yours.",
                    recommend(tuning.recommended_quality),
                ))),
                None => lines.push(Line::from(
                    "First-use tuning: not run yet — it runs automatically once audio is live.",
                )),
            }
        }
    }
    match &model.snapshot.audio_quality {
        Some(quality) => {
            let picked = crate::cli::quality_label(model.quality_choice);
            let recommendation = (model.quality_choice == quality.recommended)
                .then_some(" (the check's recommendation)");
            lines.push(Line::from(format!(
                "DSP quality: {}   Selected: {}{}   ↑ / ↓ to choose",
                crate::cli::quality_label(quality.quality),
                picked,
                recommendation.unwrap_or(""),
            )));
            lines.push(Line::from(format!(
                "  {}",
                crate::cli::quality_cost(model.quality_choice)
            )));
        }
        None => lines.push(Line::from(
            "DSP quality: unavailable — update the on-air core.",
        )),
    }
    lines.push(Line::from(
        "Enter: run ~2 second check   a: apply buffer   q: apply quality   Esc: close",
    ));
    lines.push(Line::from(
        "Check during your usual workload; settings stay unchanged until applied.",
    ));
    lines.push(Line::from(
        "Larger buffers trade more delay for tolerance of scheduling stalls.",
    ));
    if let Some(report) = &model.benchmark {
        lines.extend(
            crate::cli::benchmark_text(report)
                .lines()
                .map(|line| Line::from(line.to_string())),
        );
    }
    if model.pending.is_some() {
        lines.push(Line::from("Working…"));
    }
    if let Some(error) = &model.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::new().fg(Color::Red),
        )));
    }
    let rect = centered(area, area.width.min(92), area.height.min(22));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" Audio performance ")),
        rect,
    );
    rect
}

/// The manual latency trim of one output. The copy insists on the truth:
/// a trim is a correction typed by ear. Against an uncalibrated default it
/// is not a measurement either; against a measured latency it only nudges
/// the measured value.
pub(super) fn trim_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let Modal::TrimPrompt {
        transport,
        device_id,
        name,
        input,
        error,
    } = &model.modal
    else {
        unreachable!("trim_overlay is only called for the trim prompt");
    };
    let row = model.sync_row(transport, device_id);
    let working = model.pending.is_some();
    let mut lines = vec![Line::from(
        "Manual latency correction against what you hear, in milliseconds.",
    )];
    match row.and_then(|row| row.calibration.as_ref()) {
        Some(measurement) => {
            let ms = (measurement.latency_us + 500) / 1000;
            lines.push(Line::from(format!(
                "Measured: {ms} ms ({} confidence, {} run{}). The trim nudges the measured value, and applies the next time this output plays.",
                crate::model::confidence_word(measurement.confidence),
                measurement.runs,
                if measurement.runs == 1 { "" } else { "s" },
            )));
        }
        None => lines.push(Line::from(
            "It is not measured or calibrated, and applies the next time this output plays.",
        )),
    }
    lines.push(Line::from(match row.map(|row| row.trim_ms) {
        Some(trim) => format!("Current trim: {trim:+} ms"),
        None => "Current trim: 0 ms (not yet reported)".to_string(),
    }));
    lines.push(Line::from(Span::styled(
        format!("Trim: {input}▏"),
        Style::new().bold(),
    )));
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    if working {
        lines.push(Line::from(Span::styled(
            "Setting…",
            Style::new().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(Span::styled(
        "Enter: apply   Esc: close",
        Style::new().dim(),
    )));
    let width = area.width.min(64);
    let rect = centered(area, width, overlay_height(&lines, width, area));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Latency trim · {name} "))),
        rect,
    );
    rect
}

/// The sync-group overlay for one output: which groups exist, where this
/// output plays and the keys that change it. The copy insists on the
/// truth: a group is several speakers playing in sync, never one merged
/// device, and each member's latency figure says whether it was measured.
pub(super) fn group_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let Modal::GroupPrompt {
        transport,
        device_id,
        name,
        index,
        input,
        naming,
        error,
    } = &model.modal
    else {
        unreachable!("group_overlay is only called for the group prompt");
    };
    let groups = model.snapshot.sync_groups.as_deref().unwrap_or_default();
    let current = model
        .sync_row(transport, device_id)
        .and_then(|row| row.group_id);
    let mut lines = vec![
        Line::from("Several speakers play the same audio in sync — not one merged device."),
        Line::from("Each keeps its own volume and latency trim, typed by ear."),
        Line::from("Latency per output: measured where marked, a documented default otherwise, plus any trim."),
        Line::from(match current.and_then(|id| groups.iter().find(|g| g.id == id)) {
            Some(group) => format!("This output: {name} — in \"{}\"", group.name),
            None => format!("This output: {name} — not in a group yet"),
        }),
    ];
    if groups.is_empty() {
        lines.push(Line::from("No groups yet — press n to name one."));
    }
    for (position, group) in groups.iter().enumerate() {
        let mut listed = String::new();
        for member in &group.members {
            if !listed.is_empty() {
                listed.push_str(", ");
            }
            // A member that failed to start is marked, not hidden.
            if member.error.is_some() {
                listed.push_str("✗ ");
            }
            listed.push_str(&member.device_name);
        }
        if listed.is_empty() {
            listed.push_str("no speakers yet");
        }
        let state = if group.playing {
            "playing together"
        } else {
            "stopped"
        };
        let here = if Some(group.id) == current {
            " (this output)"
        } else {
            ""
        };
        lines.push(Line::from(format!(
            "{}{} — {listed} — {state}{here}",
            if position == *index { "> " } else { "  " },
            group.name,
        )));
    }
    if *naming {
        lines.push(Line::from(Span::styled(
            format!("New group name: {input}▏"),
            Style::new().bold(),
        )));
        lines.push(Line::from(Span::styled(
            format!("Enter: create it and put {name} in it   Esc: close"),
            Style::new().dim(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "↑/↓: choose   Enter: join or take out   n: new group   Esc: close",
            Style::new().dim(),
        )));
        lines.push(Line::from(Span::styled(
            "p: play together / stop   x: remove the group",
            Style::new().dim(),
        )));
        lines.push(Line::from(
            "Removing a group stops its speakers if they are playing along.",
        ));
    }
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    if model.pending.is_some() {
        lines.push(Line::from(Span::styled(
            "Working…",
            Style::new().fg(Color::Yellow),
        )));
    }
    let width = area.width.min(72);
    let rect = centered(area, width, overlay_height(&lines, width, area));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Sync group · {name} "))),
        rect,
    );
    rect
}

/// The acoustic calibration prompt for one output: what the run does,
/// the microphone that listens, the latency figure in effect today and —
/// once a run finishes — exactly what it measured and what the core did
/// with it. The copy insists on the truth: a documented default is never
/// a measurement, and a low-confidence result is reported, not applied.
pub(super) fn calibrate_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let Modal::Calibrate {
        transport,
        device_id,
        name,
        input,
        error,
    } = &model.modal
    else {
        unreachable!("calibrate_overlay is only called for the calibrate prompt");
    };
    let mut lines = vec![
        Line::from(format!(
            "Measure what {name} really plays: a short probe plays through THIS output, and the microphone you name below hears it arrive."
        )),
        Line::from(
            "Keep the room quiet while it runs: three probe runs, a few seconds each. Voices or music spoil the detection.",
        ),
    ];
    match model
        .sync_row(transport, device_id)
        .map(crate::model::latency_figure)
    {
        Some(figure) => lines.push(Line::from(format!("Latency now: {}", figure.describe()))),
        None => lines.push(Line::from(
            "Latency now: this core does not report one for this output.",
        )),
    }
    lines.push(Line::from(Span::styled(
        format!("Microphone: {input}▏"),
        Style::new().bold(),
    )));
    lines.push(Line::from(
        "A monitor/loopback source is refused: it would hear the transport's own digital signal and report ~0. Name a real microphone.",
    ));
    if let Some(report) = &model.calibration {
        lines.push(Line::from("Last run:"));
        lines.extend(
            crate::cli::calibration_report_text(report)
                .lines()
                .map(|line| Line::from(line.to_string())),
        );
    }
    if model.pending.is_some() {
        lines.push(Line::from(Span::styled(
            "Measuring… the probe plays three times; keep the room quiet.",
            Style::new().fg(Color::Yellow),
        )));
    }
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::new().fg(Color::Red),
        )));
    }
    lines.push(Line::from(Span::styled(
        if model.calibration.is_some() {
            "Enter: measure again   Esc: close"
        } else {
            "Enter: start measuring   Esc: close"
        },
        Style::new().dim(),
    )));
    let width = area.width.min(88);
    let rect = centered(area, width, overlay_height(&lines, width, area));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Calibrate · {name} "))),
        rect,
    );
    rect
}

pub(super) fn firewall_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let mut lines = vec![Line::from(
        "Allow local devices to reach on-air's audio port.",
    )];
    if let Some(warning) = model
        .snapshot
        .status
        .as_ref()
        .and_then(|s| s.firewall_warning.as_ref())
    {
        lines.push(Line::from(format!(
            "Allow {} → TCP {}",
            warning.subnet, warning.port
        )));
        lines.push(Line::from(
            "Run these commands on the host if automatic repair is unavailable:",
        ));
        for command in warning.repair_command.lines() {
            lines.push(Line::from(command.to_string()));
        }
    }
    lines.push(Line::from(
        if crate::handoff::is_loopback_base(&model.base) {
            "Enter: add rule (administrator authentication)   Esc: cancel"
        } else {
            "Open on-air on the host to authorize this rule. Esc: close"
        },
    ));
    if model
        .pending
        .as_ref()
        .is_some_and(|p| p.label == crate::update::LABEL_FIREWALL)
    {
        lines.push(Line::from(
            "Complete the administrator authentication prompt on this computer…",
        ));
    }
    if let Some(error) = &model.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::new().fg(Color::Red),
        )));
    }
    let rect = centered(area, area.width.min(90), area.height.min(16));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" Firewall help ")),
        rect,
    );
    rect
}

/// The port the core answers on, from its base URL.
fn core_port(base: &str) -> u16 {
    reqwest::Url::parse(base)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(DEFAULT_PORT)
}

pub(super) fn status_overlay(model: &Model, frame: &mut Frame, area: Rect) -> Rect {
    let status = model.snapshot.status.as_ref();
    let port = core_port(&model.base);
    let yes_no = |value: Option<bool>| match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    let on_off = |value: Option<bool>| match value {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    };
    let mut lines = vec![
        Line::from(format!(
            "Version: {}   Port: {port}",
            status.map_or("unknown", |s| s.version.as_str())
        )),
        Line::from("Addresses:"),
    ];
    match status.and_then(|s| s.lan_addresses.as_ref()) {
        Some(addresses) if !addresses.is_empty() => lines.extend(
            addresses
                .iter()
                .map(|address| Line::from(format!("  {address}:{port}"))),
        ),
        _ => lines.push(Line::from("  none found")),
    }
    lines.extend([
        Line::from(format!(
            "AirPlay: {}",
            model.snapshot.airplay_mode.as_deref().unwrap_or("unknown")
        )),
        Line::from(if model.service_on() {
            "monitor on"
        } else {
            "monitor off"
        }),
        Line::from(format!(
            "sleep inhibited: {}",
            yes_no(status.and_then(|s| s.sleep_inhibited))
        )),
        Line::from(format!("login start: {}", on_off(model.autostart))),
        Line::from(format!("Core: {}", model.base)),
        Line::from(Span::styled(" Esc or i closes ", Style::new().dim())),
    ]);
    let rect = centered(
        area,
        area.width.min(60),
        (lines.len() as u16 + 2).min(area.height),
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Status ")),
        rect,
    );
    rect
}
