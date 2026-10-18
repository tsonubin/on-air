//! C6 first-run tuning at the watcher level: the automatic check runs once
//! on a machine that has never been checked, never again afterwards, and
//! skips cleanly where it cannot run (mock mode, no capture).
//!
//! The transition rules themselves live in `on_air_core::tuning::decide`
//! and are pinned by its unit tests; what is exercised here is that the
//! watcher task follows them without blocking startup and without touching
//! a machine that is already tuned.
use on_air_core::dsp::DspQuality;
use on_air_core::state::CoreState;
use on_air_core::tuning;
use std::time::Duration;
use tower::ServiceExt;

async fn set_settings(state: &CoreState, buffer_ms: u32, quality: &str) {
    let app = on_air_core::build_router(state.clone());
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::put("/api/audio/buffer")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    "{{\"buffer_ms\":{buffer_ms}}}"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
    let response = app
        .oneshot(
            axum::http::Request::put("/api/audio/quality")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    "{{\"quality\":\"{quality}\"}}"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn mock_mode_skips_cleanly_and_leaves_the_machine_pending() {
    let state = CoreState::new_mock().await;
    state.input().activate("Mock Monitor").await.unwrap();
    let watcher = tuning::spawn_first_run_watcher(state.clone());
    // No check runs (nothing to measure on a mock core) and the machine
    // stays pending, so a real machine retries later.
    tokio::time::timeout(Duration::from_secs(2), watcher)
        .await
        .expect("the watcher skips mock mode immediately")
        .unwrap();
    assert!(state.first_run_tuning_pending());
    assert_eq!(state.saved_settings().first_run_tuning, None);
}

#[tokio::test]
async fn a_machine_that_has_been_checked_is_never_tuned_again() {
    let state = CoreState::new_mock().await;
    state.record_check_verdict(DspQuality::Economy);
    let watcher = tuning::spawn_first_run_watcher(state.clone());
    tokio::time::timeout(Duration::from_secs(2), watcher)
        .await
        .expect("the watcher leaves a tuned machine alone")
        .unwrap();
    assert!(!state.first_run_tuning_pending());
    assert_eq!(state.saved_settings().first_run_tuning, None, "nothing ran");
}

#[tokio::test]
async fn without_a_live_capture_the_watcher_waits_instead_of_running() {
    let state = CoreState::new();
    let watcher = tuning::spawn_first_run_watcher(state.clone());
    // Nothing is capturing: the check must not run before capture can
    // work, so the watcher is still waiting a moment later.
    let finished = tokio::time::timeout(Duration::from_millis(500), watcher).await;
    assert!(finished.is_err(), "the watcher ran without a live capture");
    assert!(state.first_run_tuning_pending());
}

#[tokio::test]
async fn first_use_runs_the_real_check_once_and_records_its_verdict() {
    // A core whose mock input is live but which believes it is on real
    // hardware: the tuner runs for real (the check itself is synthetic),
    // records its verdict, keeps the settings the user already chose, and
    // never runs again.
    let mut state = CoreState::new_mock().await;
    state.input().activate("Mock Monitor").await.unwrap();
    set_settings(&state, 200, "high").await;
    state.set_mock(false);

    let watcher = tuning::spawn_first_run_watcher(state.clone());
    tokio::time::timeout(Duration::from_secs(30), watcher)
        .await
        .expect("the first-use check completes without blocking anything")
        .unwrap();

    let saved = state.saved_settings();
    assert!(!state.first_run_tuning_pending(), "the check completed");
    assert!(
        saved.recommended_dsp_quality.is_some(),
        "the verdict is recorded"
    );
    assert_eq!(saved.capture_buffer_ms, 200, "the user's buffer is kept");
    assert_eq!(
        saved.dsp_quality,
        Some(DspQuality::High),
        "the user's quality is kept"
    );
    let record = saved.first_run_tuning.expect("the automatic run is recorded");
    assert!(record.recommended_buffer_ms >= 50);
    assert!(!record.applied_buffer && !record.applied_quality);

    // The second boot: the watcher exits without doing anything.
    let watcher = tuning::spawn_first_run_watcher(state.clone());
    tokio::time::timeout(Duration::from_secs(2), watcher)
        .await
        .expect("no second run")
        .unwrap();
}
