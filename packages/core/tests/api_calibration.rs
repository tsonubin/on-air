//! Acoustic calibration over the API (roadmap B10), end to end on the mock
//! core. No audio hardware exists here, so the mock loopback injects a
//! KNOWN synthetic delay through the probe→detect pipeline: what must come
//! out the other side is that delay — measured, applied to the sync
//! machinery (`calibrated` flips), persisted across a restart, and used in
//! place of the documented default.
//!
//! The estimator's accuracy against noisy, attenuated, coloured and
//! fractional-delay inputs is pinned in `calibration::detect`'s tests;
//! these cover the product path around it.

mod common;
use common::*;

use axum::http::StatusCode;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn scratch_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "on-air-calibration-{name}-{}-{nonce}.json",
        std::process::id()
    ))
}

/// Poll `GET /api/sync/calibration` until the run finishes and return the
/// completed run.
async fn wait_for_run(app: &axum::Router) -> serde_json::Value {
    for _ in 0..600 {
        let (status, body) = send(app.clone(), get("/api/sync/calibration")).await;
        assert_eq!(status, StatusCode::OK);
        if body["active"].is_null() && !body["last"].is_null() {
            return body["last"].clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the calibration run did not finish");
}

const RUN_REQUEST: &str = r#"{
    "targets": [{"transport": "sonos", "device_id": "uuid:mock-sonos"}],
    "capture_input": "Mock Microphone",
    "runs_per_target": 3,
    "max_delay_ms": 4000
}"#;

#[tokio::test]
async fn a_measured_latency_replaces_the_default_persists_and_is_used() {
    let path = scratch_path("e2e");
    let state = on_air_core::state::CoreState::new_mock_persistent(&path).await;
    let app = on_air_core::build_router(state.clone());

    // The synthetic acoustic world: a 1234.567 ms arrival, attenuated and
    // colouration-filtered, with 2 ms of capture arrival jitter so the
    // sample-index fit does real work. The linear-phase FIR's group delay
    // (3 samples at 48 kHz = 62.5 µs) is part of the expected arrival.
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/mock/calibration/loopback",
            r#"{
                "delay_us": 1234567.7,
                "noise_rms": 0.002,
                "gain": 0.5,
                "colouration": true,
                "capture_rate_hz": 48000,
                "jitter_us": 2000
            }"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, started) = send(app.clone(), post_json("/api/sync/calibrate", RUN_REQUEST)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(started["state"], "running");
    assert_eq!(started["targets"].as_array().unwrap().len(), 1);

    let run = wait_for_run(&app).await;
    assert_eq!(run["state"], "complete", "one target measured and applied");
    let target = &run["targets"][0];
    assert_eq!(target["state"], "complete", "error: {}", target["error"]);
    let measurement = &target["measurement"];
    let latency_us = measurement["latency_us"].as_f64().unwrap();
    // The injected arrival plus the colouration FIR's group delay: 3
    // samples at 48 kHz.
    let expected_us = 1_234_567.7 + 3.0 / 48_000.0 * 1_000_000.0;
    println!(
        "end-to-end: measured {latency_us} µs against injected {expected_us} µs \
         (error {} µs)",
        (latency_us - expected_us).abs()
    );
    assert!(
        (latency_us - expected_us).abs() < 1_000.0,
        "measured {latency_us} µs, injected {expected_us} µs"
    );
    assert_eq!(measurement["runs"], 3, "three runs, three trusted arrivals");
    assert_eq!(measurement["run_delays_us"].as_array().unwrap().len(), 3);
    assert!(measurement["samples"].as_u64().unwrap() > 0);
    assert_eq!(measurement["sample_rate_hz"], 48_000);
    let confidence = measurement["confidence"].as_str().unwrap();
    assert!(
        confidence == "high" || confidence == "medium",
        "a clean synthetic world must be trustworthy, got {confidence}"
    );

    // The measurement is applied: GET /api/sync shows measured vs default.
    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let sonos = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "uuid:mock-sonos")
        .expect("the measured output is reported");
    assert_eq!(sonos["calibrated"], true, "a measurement flips calibrated");
    assert_eq!(
        sonos["default_latency_ms"], 1_000,
        "the guess is still shown"
    );
    let effective = sonos["effective_latency_ms"].as_f64().unwrap();
    assert!(
        (effective - latency_us / 1000.0).abs() <= 1.0,
        "the measurement, not the 1000 ms default, is used: {effective}"
    );
    assert_eq!(
        sonos["calibration"]["latency_us"],
        measurement["latency_us"]
    );

    // And it is used by the sync machinery: the Sonos member's delay line
    // is sized from the measurement, not from the 1000 ms placeholder. A
    // group of the measured Sonos (≈1234.6 ms) and a default AirPlay
    // (2000 ms) holds the fast member back the difference.
    let group = state.sync.create_group("Downstairs");
    state
        .sync
        .join(
            &group,
            &on_air_core::state::ActiveOutput {
                transport: "sonos".into(),
                device_id: "uuid:mock-sonos".into(),
                device_name: "Mock Sonos".into(),
            },
        )
        .unwrap();
    state
        .sync
        .join(
            &group,
            &on_air_core::state::ActiveOutput {
                transport: "airplay".into(),
                device_id: "ap-living".into(),
                device_name: "Living Room AirPlay".into(),
            },
        )
        .unwrap();
    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let outputs = body["outputs"].as_array().unwrap();
    let slow = outputs
        .iter()
        .find(|o| o["device_id"] == "ap-living")
        .unwrap();
    let fast = outputs
        .iter()
        .find(|o| o["device_id"] == "uuid:mock-sonos")
        .unwrap();
    assert_eq!(
        slow["delay_frames"], 0,
        "the slowest member waits for nobody"
    );
    // Nothing is live, so the sync status sizes delay lines at its fallback
    // rate of 48 kHz.
    let rate = 48_000.0;
    let expected_frames = ((2_000_000.0 - latency_us) / 1_000_000.0 * rate).round() as i64;
    let got_frames = fast["delay_frames"].as_i64().unwrap();
    assert!(
        (got_frames - expected_frames).abs() <= 2,
        "delay line from the measurement: got {got_frames}, expected ~{expected_frames} \
         (the 1000 ms default would give 48000)"
    );

    // Persisted: a restart sees the measurement, not the default.
    drop(app);
    drop(state);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let state = on_air_core::state::CoreState::new_mock_persistent(&path).await;
    let app = on_air_core::build_router(state.clone());
    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    let sonos = body["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["device_id"] == "uuid:mock-sonos")
        .unwrap();
    assert_eq!(
        sonos["calibrated"], true,
        "the measurement survived a restart"
    );
    assert_eq!(
        sonos["calibration"]["latency_us"], measurement["latency_us"],
        "exactly the figure that was measured"
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn a_bad_acoustic_world_is_reported_not_trusted() {
    let state = on_air_core::state::CoreState::new_mock().await;
    let app = on_air_core::build_router(state.clone());

    // Pure noise: no probe can survive this. The run must finish with a
    // report, not with a confident wrong number, and the default stays.
    let (status, _) = send(
        app.clone(),
        post_json(
            "/api/mock/calibration/loopback",
            r#"{"delay_us": 150000.0, "noise_rms": 0.5, "gain": 0.001, "capture_rate_hz": 48000}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send(app.clone(), post_json("/api/sync/calibrate", RUN_REQUEST)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let run = wait_for_run(&app).await;
    let target = &run["targets"][0];
    assert_eq!(target["state"], "failed");
    assert!(
        target["error"].as_str().unwrap().contains("probe arrival"),
        "the report says plainly what went wrong: {}",
        target["error"]
    );
    assert!(
        target["measurement"].is_null(),
        "no measurement is invented from garbage"
    );
    assert_eq!(run["state"], "failed");

    let (status, body) = send(app.clone(), get("/api/sync")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|output| output["calibrated"] == false),
        "a bad run never calibrates anything"
    );
    assert!(
        state
            .sync
            .calibration(&on_air_core::sync::OutputKey::new(
                "sonos",
                "uuid:mock-sonos"
            ))
            .is_none(),
        "and the default is kept, not replaced by noise"
    );
    let _ = app;
}

#[tokio::test]
async fn a_loopback_capture_input_is_refused_even_in_mock() {
    // The guard is production logic: it must reject a monitor source at
    // the API, so a real run can never "measure" a transport against its
    // own digital monitor. (Mock runs only synthesize; the guard still
    // applies to the request shape.)
    let state = on_air_core::state::CoreState::new_mock().await;
    let app = on_air_core::build_router(state);
    let (status, body) = send(
        app,
        post_json(
            "/api/sync/calibrate",
            r#"{
                "targets": [{"transport": "sonos", "device_id": "uuid:mock-sonos"}],
                "capture_input": "BlackHole 2ch"
            }"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("monitor/loopback source"));
}

#[tokio::test]
async fn the_synthetic_loopback_does_not_exist_on_a_production_core() {
    // A production core (mock off) never routes the mock world: there is
    // no synthetic path a real run could accidentally take.
    let state = on_air_core::state::CoreState::new();
    let app = on_air_core::build_router(state);
    let (status, _) = send(
        app,
        post_json(
            "/api/mock/calibration/loopback",
            r#"{"delay_us": 150000.0}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_calibration_request_validates_its_shape() {
    let state = on_air_core::state::CoreState::new_mock().await;
    let app = on_air_core::build_router(state);
    for body in [
        r#"{"targets": [], "capture_input": "Mic"}"#,
        r#"{"targets": [{"transport": "carrier-pigeon", "device_id": "x"}], "capture_input": "Mic"}"#,
        r#"{"targets": [{"transport": "sonos", "device_id": "x"}], "capture_input": ""}"#,
        r#"{"targets": [{"transport": "sonos", "device_id": "x"}], "capture_input": "Mic", "runs_per_target": 0}"#,
        r#"{"targets": [{"transport": "sonos", "device_id": "x"}], "capture_input": "Mic", "max_delay_ms": 10}"#,
    ] {
        let (status, _) = send(app.clone(), post_json("/api/sync/calibrate", body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "must reject {body}");
    }
}
