//! The help, pairing and CD overlays.

use super::layout::centered;
use crate::keys::KEYS;
use crate::model::Model;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// The key table, then the mouse rows, so the help still fits 100×30.
pub(super) fn help_overlay(frame: &mut Frame, area: Rect, mouse: bool) -> Rect {
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
        Paragraph::new(lines).block(Block::bordered().title(" Keys ")),
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
/// a trim is a correction typed by ear, never a calibrated measurement.
pub(super) fn trim_overlay(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    current: Option<i32>,
    input: &str,
    error: Option<&str>,
    working: bool,
) -> Rect {
    let mut lines = vec![
        Line::from("Manual latency correction against what you hear, in milliseconds."),
        Line::from("It is not measured or calibrated, and applies the next time this output plays."),
    ];
    lines.push(Line::from(match current {
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
    // Two spare rows over the logical lines: the copy wraps at overlay
    // widths and a clipped input line would hide the control.
    let height = lines.len() as u16 + 4;
    let rect = centered(area, area.width.min(64), height.min(area.height));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(format!(" Latency trim · {name} "))),
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
