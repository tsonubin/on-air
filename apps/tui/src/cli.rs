//! The subcommands: `on-air bar` for the bar's command module, `on-air
//! status` for humans and scripts, `on-air monitor` for menu rows. They
//! share the client with the TUI so there is no shell script to keep in
//! step with the API.

use crate::api::{
    ActiveOutputView, CalibrationConfidence, CalibrationRunView, CalibrationTarget,
    CalibrationTargetState, CalibrationTargetStatus, DspQuality, StatusResponse,
};
use crate::client::{Client, ClientError};
use crate::model::{confidence_word, TRANSPORTS};
use serde::Serialize;

/// How each DSP quality profile is named in the UIs.
pub fn quality_label(quality: DspQuality) -> &'static str {
    match quality {
        DspQuality::Economy => "Economy",
        DspQuality::Balanced => "Balanced",
        DspQuality::High => "High",
    }
}

/// What each DSP quality profile costs: the resampler behind it and its
/// CPU price.
pub fn quality_cost(quality: DspQuality) -> &'static str {
    match quality {
        DspQuality::Economy => "Cubic (Farrow) resampler: the cheapest, with rougher conversion.",
        DspQuality::Balanced => "128-tap sinc resampler: clean conversion at moderate CPU cost.",
        DspQuality::High => {
            "256-tap sinc resampler: the cleanest conversion, the highest CPU cost."
        }
    }
}

const GLYPH_ON: &str = "\u{f075a}"; // nf-md-speaker
const GLYPH_OFF: &str = "\u{f075b}"; // nf-md-speaker_off

pub async fn firewall(client: &Client, allow: bool) -> Result<String, ClientError> {
    let status = client.status().await?;
    let Some(warning) = status.firewall_warning else {
        return Ok("No recent UFW block affecting on-air was observed. Detection requires access to kernel logs.".into());
    };
    if allow {
        println!(
            "Allowing {} to reach TCP {}. Complete administrator authentication on the host.",
            warning.subnet, warning.port
        );
        client.repair_firewall().await?;
        Ok("Scoped LAN audio rule applied.".into())
    } else {
        Ok(format!(
            "{}\nAllow {} → TCP {}.\nOn the host: on-air firewall allow\nOr run:\n{}",
            warning.message, warning.subnet, warning.port, warning.repair_command
        ))
    }
}

pub fn benchmark_text(report: &crate::api::AudioBenchmark) -> String {
    format!("Audio check: {} → {} Hz\nPipeline p95: {:.1} ms; scheduler p95: {:.1} ms\nWorst round trip: {:.1} ms\nRecommended capture buffer: {} ms (3× timing margin + one audio quantum)\nRecommended DSP quality: {} — {}\n{}\nRun during your usual workload. This does not measure USB capture errors or network/speaker buffering.",
        report.input_hz, report.output_hz, report.pipeline_p95_ms, report.scheduler_p95_ms,
        report.worst_case_ms, report.recommended_buffer_ms,
        quality_label(report.recommended_quality), quality_cost(report.recommended_quality),
        if report.within_buffer_budget { "System priority stays unchanged." } else { "Measured stalls exceed the available buffer budget. Reduce load or sample rate and retry." })
}

pub async fn audio(
    client: &Client,
    action: &str,
    value: Option<u32>,
    json: bool,
) -> Result<String, ClientError> {
    if action == "check" {
        let report = client.audio_check().await?;
        return Ok(if json {
            serde_json::to_string_pretty(&report).unwrap()
        } else {
            format!(
                "{}\nApply with: on-air audio buffer {}",
                benchmark_text(&report),
                report.recommended_buffer_ms
            )
        });
    }
    if let Some(value) = value {
        client.set_capture_buffer(value).await?;
    }
    let buffer = client.status().await?.capture_buffer_ms.unwrap_or(0);
    Ok(if json {
        format!("{{\"capture_buffer_ms\":{buffer}}}")
    } else if buffer == 0 {
        "Capture buffer: system default".into()
    } else {
        format!("Requested capture buffer: {buffer} ms")
    })
}

/// A microsecond figure as milliseconds, for report lines.
fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

/// One target's share of a finished run's report: what was measured,
/// whether the core applied it, and each run's own delay. A not-applied
/// measurement says so; a default is never presented as a measurement.
fn target_report(target: &CalibrationTargetStatus) -> String {
    let name = &target.device_name;
    let mut lines = Vec::new();
    match &target.measurement {
        Some(measurement) => {
            let tail = if target.state == CalibrationTargetState::Complete {
                "applied"
            } else if measurement.confidence == CalibrationConfidence::Low {
                "low confidence — not applied"
            } else {
                "not applied"
            };
            lines.push(format!(
                "{name}: measured {:.1} ms ({} confidence, {} run{}, spread {:.1} ms) — {tail}",
                ms(measurement.latency_us),
                confidence_word(measurement.confidence),
                measurement.runs,
                if measurement.runs == 1 { "" } else { "s" },
                ms(measurement.spread_us),
            ));
            if !measurement.run_delays_us.is_empty() {
                let delays: Vec<String> = measurement
                    .run_delays_us
                    .iter()
                    .map(|us| format!("{:.1}", ms(*us)))
                    .collect();
                lines.push(format!("  runs: {} ms", delays.join(", ")));
            }
        }
        None => lines.push(format!("{name}: no measurement — not applied")),
    }
    if let Some(error) = &target.error {
        lines.push(format!("  {error}"));
    }
    lines.join("\n")
}

/// The final per-target report of a finished calibration run, shared by
/// `on-air calibrate` and the TUI's calibrate overlay.
pub fn calibration_report_text(run: &CalibrationRunView) -> String {
    run.targets
        .iter()
        .map(target_report)
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `on-air calibrate` hands back: what to print, and whether any
/// target earned an applied measurement. The exit code follows `applied`.
pub struct CalibrateReport {
    pub output: String,
    pub applied: bool,
}

/// `on-air calibrate`: mic-loopback measurement of one output's — or every
/// known output's — playout latency. The probe plays through each target
/// in turn and the named microphone listens; `--json` prints the finished
/// run instead of the report.
pub async fn calibrate(
    client: &Client,
    target: Option<(&str, &str)>,
    input: Option<&str>,
    runs: Option<u32>,
    json: bool,
) -> Result<CalibrateReport, String> {
    let copy = crate::error_copy::calibration_copy;
    let capture_input = match input {
        Some(input) => input.to_string(),
        // The active source is the natural default, unless it cannot hear
        // a probe: a monitor/loopback is refused by the core anyway.
        None => match client
            .active_input()
            .await
            .map_err(|error| copy(&error))?
            .name
        {
            Some(name) if crate::labels::is_monitor_source(&name) => {
                return Err(format!(
                    "The active source \"{name}\" is a monitor/loopback and cannot hear the probe. Name a real microphone with --input."
                ));
            }
            Some(name) => name,
            None => {
                return Err("No capture source is active. Name the microphone with --input.".into())
            }
        },
    };
    let targets = match target {
        Some((transport, device_id)) => {
            if transport.is_empty() || device_id.is_empty() {
                return Err("calibrate needs a transport and a device id, or --all".into());
            }
            vec![CalibrationTarget {
                transport: transport.to_string(),
                device_id: device_id.to_string(),
            }]
        }
        // "All transports" is one target per output in the catalog,
        // local ones included; runs are sequential server-side.
        None => {
            let outputs = client.outputs().await.map_err(|error| copy(&error))?;
            let targets: Vec<CalibrationTarget> = outputs
                .into_iter()
                .filter(|output| !output.id.is_empty())
                .map(|output| CalibrationTarget {
                    transport: output.transport,
                    device_id: output.id,
                })
                .collect();
            if targets.is_empty() {
                return Err("No outputs are known yet — connect one first.".into());
            }
            // The core measures at most 16 targets per run.
            if targets.len() > 16 {
                return Err(format!(
                    "The core measures at most 16 outputs per run ({} known); calibrate them in batches.",
                    targets.len()
                ));
            }
            targets
        }
    };
    let runs_per_target = runs.unwrap_or(crate::client::CALIBRATION_RUNS);
    if !json {
        let scope = match target {
            Some((transport, device_id)) => format!("{transport}/{device_id}"),
            None => format!("{} outputs, one after another", targets.len()),
        };
        println!(
            "Measuring latency of {scope} on microphone \"{capture_input}\", {runs_per_target} probe run(s) each. Keep the room quiet: the probe plays through every output being measured."
        );
    }
    client
        .start_calibration(&targets, &capture_input, runs_per_target)
        .await
        .map_err(|error| copy(&error))?;
    let run = client
        .wait_for_calibration()
        .await
        .map_err(|error| copy(&error))?;
    let applied = run
        .targets
        .iter()
        .any(|target| target.state == CalibrationTargetState::Complete);
    Ok(CalibrateReport {
        output: if json {
            serde_json::to_string_pretty(&run).unwrap()
        } else {
            calibration_report_text(&run)
        },
        applied,
    })
}

/// One Waybar-style line for a bar module: `{"text","tooltip","class"}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BarLine {
    pub text: String,
    pub tooltip: String,
    pub class: &'static str,
}

pub fn bar_line(
    status: Option<&StatusResponse>,
    active: Option<&ActiveOutputView>,
    volume: Option<u8>,
) -> BarLine {
    let Some(status) = status else {
        return BarLine {
            text: GLYPH_OFF.to_string(),
            tooltip: "on-air-core is not running".to_string(),
            class: "down",
        };
    };
    if !status.service_enabled {
        return BarLine {
            text: format!("{GLYPH_OFF} off"),
            tooltip: "on-air monitor is off".to_string(),
            class: "off",
        };
    }
    match active.filter(|active| active.is_live()) {
        Some(active) => {
            let transport = TRANSPORTS
                .iter()
                .find(|(id, _)| *id == active.output.transport)
                .map(|(_, label)| *label)
                .unwrap_or("Output");
            let mut tooltip = format!("{transport} · {}", active.output.device_name);
            if let Some(volume) = volume {
                tooltip.push_str(&format!(" · vol {volume}"));
            }
            if let Some(address) = status.lan_addresses.as_ref().and_then(|a| a.first()) {
                tooltip.push_str(&format!(" · {address}"));
            }
            BarLine {
                text: format!("{GLYPH_ON} {}", active.output.device_name),
                tooltip,
                class: "live",
            }
        }
        None => BarLine {
            text: GLYPH_ON.to_string(),
            tooltip: "on-air: no speaker selected".to_string(),
            class: "idle",
        },
    }
}

/// Never fails: a bar module must always print something.
pub async fn bar(client: &Client) -> String {
    let (status, active, volume) =
        tokio::join!(client.status(), client.active_output(), client.volume());
    let line = bar_line(
        status.as_ref().ok(),
        active.as_ref().ok().and_then(|a| a.as_ref()),
        volume.ok(),
    );
    serde_json::to_string(&line).unwrap_or_else(|_| "{\"text\":\"on-air\"}".to_string())
}

pub async fn status(client: &Client, json: bool) -> Result<String, ClientError> {
    let (snapshot, failures) = client.load_snapshot().await;
    let Some(status) = &snapshot.status else {
        return Err(ClientError::Unreachable(client.base().to_string()));
    };
    if json {
        return Ok(serde_json::json!({
            "status": status,
            "pin": snapshot.pin,
            "inputs": snapshot.inputs,
            "active_input": snapshot.active_input,
            "outputs": snapshot.outputs,
            "active_output": snapshot.active_output,
            "volume": snapshot.volume,
            "eq_gains_db": snapshot.eq,
            "sample_rate": snapshot.rates,
            "cd": snapshot.cd,
            "airplay_mode": snapshot.airplay_mode,
            "failures": failures,
        })
        .to_string());
    }
    let mut lines = vec![
        format!("core: {} ({})", client.base(), status.version),
        format!(
            "monitor: {}",
            if status.service_enabled { "on" } else { "off" }
        ),
        format!(
            "sleep inhibited: {}",
            match status.sleep_inhibited {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            }
        ),
    ];
    if let Some(addresses) = &status.lan_addresses {
        lines.push(format!("lan: {}", addresses.join(", ")));
    }
    if let Some(pin) = &snapshot.pin {
        lines.push(format!("pin: {pin}"));
    }
    lines.push(format!(
        "source: {}",
        snapshot.active_input.as_deref().unwrap_or("none")
    ));
    lines.push(match &snapshot.active_output {
        Some(active) => format!(
            "output: {} ({}{})",
            active.output.device_name,
            active.output.transport,
            if active.is_live() {
                ", live"
            } else {
                ", starting"
            }
        ),
        None => "output: none".to_string(),
    });
    lines.push(format!("volume: {}", snapshot.volume));
    lines.push(format!(
        "eq: {}",
        snapshot
            .eq
            .iter()
            .map(|g| format!("{g:+.1}"))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    if let Some(rates) = &snapshot.rates {
        lines.push(format!(
            "rates: in {} Hz, out {} Hz",
            rates.input.sample_rate_hz, rates.output.sample_rate_hz
        ));
    }
    if let Some(cd) = snapshot.cd.as_ref().filter(|cd| cd.present) {
        lines.push(format!(
            "cd: track {}/{} {}",
            cd.track,
            cd.track_count,
            if cd.playing { "playing" } else { "paused" }
        ));
    }
    for failure in failures {
        lines.push(format!("warning: {failure}"));
    }
    Ok(lines.join("\n"))
}

/// `on | off | toggle`; prints the resulting state.
pub async fn monitor(client: &Client, arg: &str) -> Result<String, ClientError> {
    let enabled = match arg {
        "on" => true,
        "off" => false,
        "toggle" => !client.status().await?.service_enabled,
        other => {
            return Err(ClientError::Http {
                status: 400,
                code: Some("invalid_request".into()),
                message: format!("expected on, off or toggle, got {other:?}"),
            })
        }
    };
    let enabled = client.set_service(enabled).await?;
    Ok(format!("monitor {}", if enabled { "on" } else { "off" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ActiveOutput, CalibrationMeasurement, CalibrationRunState, OutputPhase};

    fn measurement(
        latency_us: u64,
        confidence: CalibrationConfidence,
        runs: u32,
        spread_us: u64,
        run_delays_us: Vec<u64>,
    ) -> CalibrationMeasurement {
        CalibrationMeasurement {
            transport: "sonos".into(),
            device_id: "s1".into(),
            latency_us,
            confidence,
            runs,
            samples: 48000,
            spread_us,
            sample_rate_hz: 48000,
            measured_at_unix_s: 0,
            run_delays_us,
        }
    }

    fn target(
        name: &str,
        state: CalibrationTargetState,
        measurement: Option<CalibrationMeasurement>,
        error: Option<&str>,
    ) -> CalibrationTargetStatus {
        CalibrationTargetStatus {
            transport: "sonos".into(),
            device_id: name.to_lowercase().replace(' ', "-"),
            device_name: name.into(),
            state,
            measurement,
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn the_calibration_report_says_what_was_measured_and_what_was_applied() {
        let run = CalibrationRunView {
            run_id: 7,
            state: CalibrationRunState::Complete,
            targets: vec![
                target(
                    "Mock Sonos",
                    CalibrationTargetState::Complete,
                    Some(measurement(
                        1_234_600,
                        CalibrationConfidence::High,
                        3,
                        200,
                        vec![1_234_500, 1_234_600, 1_234_700],
                    )),
                    None,
                ),
                target(
                    "Kitchen",
                    CalibrationTargetState::Failed,
                    Some(measurement(
                        812_300,
                        CalibrationConfidence::Low,
                        3,
                        4_200,
                        vec![812_000, 812_300, 816_200],
                    )),
                    Some("the runs disagreed badly; the default stays"),
                ),
                target(
                    "Bedroom",
                    CalibrationTargetState::Failed,
                    None,
                    Some("no trustworthy probe arrival was found"),
                ),
            ],
            started_at_unix_s: 0,
        };
        let text = calibration_report_text(&run);
        for needle in [
            "Mock Sonos: measured 1234.6 ms (high confidence, 3 runs, spread 0.2 ms) — applied",
            "runs: 1234.5, 1234.6, 1234.7 ms",
            "Kitchen: measured 812.3 ms (low confidence, 3 runs, spread 4.2 ms) — low confidence — not applied",
            "  the runs disagreed badly; the default stays",
            "Bedroom: no measurement — not applied",
            "  no trustworthy probe arrival was found",
        ] {
            assert!(text.contains(needle), "{needle} missing:\n{text}");
        }
    }

    fn status(enabled: bool) -> StatusResponse {
        StatusResponse {
            status: "ok".into(),
            version: "0.1.0".into(),
            service_enabled: enabled,
            sleep_inhibited: Some(enabled),
            lan_addresses: Some(vec!["10.0.0.12".into()]),
            firewall_warning: None,
            capture_buffer_ms: None,
        }
    }

    fn active(name: &str, phase: OutputPhase) -> ActiveOutputView {
        ActiveOutputView {
            output: ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:mock-sonos".into(),
                device_name: name.into(),
            },
            state: Some(phase),
        }
    }

    #[test]
    fn a_missing_core_is_down() {
        let line = bar_line(None, None, None);
        assert_eq!(line.text, "󰝛");
        assert_eq!(line.class, "down");
        assert_eq!(line.tooltip, "on-air-core is not running");
    }

    #[test]
    fn a_paused_monitor_is_off() {
        let line = bar_line(Some(&status(false)), None, None);
        assert_eq!(line.text, "󰝛 off");
        assert_eq!(line.class, "off");
        assert_eq!(line.tooltip, "on-air monitor is off");
    }

    #[test]
    fn no_live_output_is_idle() {
        let line = bar_line(Some(&status(true)), None, Some(50));
        assert_eq!(line.text, "󰝚");
        assert_eq!(line.class, "idle");
        let starting = active("Mock Sonos", OutputPhase::Starting);
        let line = bar_line(Some(&status(true)), Some(&starting), Some(50));
        assert_eq!(line.class, "idle", "starting is not live yet");
    }

    #[test]
    fn a_live_output_names_the_device() {
        let live = active("Living Room", OutputPhase::Live);
        let line = bar_line(Some(&status(true)), Some(&live), Some(80));
        assert_eq!(line.text, "󰝚 Living Room");
        assert_eq!(line.class, "live");
        assert_eq!(line.tooltip, "Sonos · Living Room · vol 80 · 10.0.0.12");
    }

    #[test]
    fn odd_device_names_still_serialise_to_valid_json() {
        let live = active("Caf\u{e9} \"Loud\"", OutputPhase::Live);
        let line = bar_line(Some(&status(true)), Some(&live), Some(1));
        let json = serde_json::to_string(&line).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["text"]
            .as_str()
            .unwrap()
            .ends_with("Caf\u{e9} \"Loud\""));
    }
}
