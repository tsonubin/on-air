use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use on_air_core::{
    audio_benchmark::{recommendation, run},
    state::CoreState,
};
use tower::ServiceExt;

#[test]
fn recommendation_leaves_three_times_observed_stalls_plus_one_quantum() {
    assert_eq!(recommendation(0.0, 23.0), (50, true));
    assert_eq!(recommendation(20.0, 23.0), (100, true));
    assert_eq!(recommendation(50.0, 23.0), (200, true));
    assert_eq!(recommendation(100.0, 23.0), (400, true));
    assert_eq!(recommendation(200.0, 23.0), (400, false));
}

#[test]
fn capture_buffer_is_applied_in_frames_and_respects_backend_limits() {
    use cpal::{BufferSize, SupportedBufferSize};
    use on_air_core::pipeline::capture::capture_buffer_size;
    let supported = SupportedBufferSize::Range {
        min: 128,
        max: 8192,
    };
    assert_eq!(
        capture_buffer_size(48_000, 100, &supported),
        BufferSize::Fixed(4800)
    );
    assert_eq!(
        capture_buffer_size(48_000, 400, &supported),
        BufferSize::Fixed(8192)
    );
    assert_eq!(
        capture_buffer_size(48_000, 0, &supported),
        BufferSize::Default
    );
    assert_eq!(
        capture_buffer_size(48_000, 100, &SupportedBufferSize::Unknown),
        BufferSize::Fixed(4800)
    );
}

#[test]
fn the_real_pipeline_check_returns_measurements_without_hardware() {
    use on_air_core::dsp::DspQuality;
    let outcome = run(
        44_100,
        48_000,
        [2.0, 0.0, 0.0, 0.0, -2.0],
        8,
        DspQuality::Balanced,
    )
    .unwrap();
    let report = &outcome.report;
    assert_eq!(report.input_hz, 44_100);
    assert_eq!(report.output_hz, 48_000);
    assert!(report.duration_ms > 0);
    assert!(report.pipeline_p95_ms.is_finite());
    assert!(report.scheduler_p95_ms >= 0.0);
    assert!([50, 100, 200, 400].contains(&report.recommended_buffer_ms));
    // The measured per-profile bridge costs are real numbers, and the
    // recommendation is one of the three profiles.
    assert!(outcome.profile_costs_ms.iter().all(|ms| ms.is_finite()));
    assert!(DspQuality::ALL.contains(&outcome.recommended_quality));
}

fn buffer_request(ms: u32) -> Request<Body> {
    Request::put("/api/audio/buffer")
        .header("content-type", "application/json")
        .body(Body::from(format!("{{\"buffer_ms\":{ms}}}")))
        .unwrap()
}

#[tokio::test]
async fn changing_buffer_keeps_the_input_selected_and_rejects_invalid_values() {
    let state = CoreState::new_mock().await;
    state.input().activate("Mock Monitor").await.unwrap();
    let app = on_air_core::build_router(state.clone());
    for ms in [50, 200, 400, 0] {
        let response = app.clone().oneshot(buffer_request(ms)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(on_air_core::status_for(&state).capture_buffer_ms, Some(ms));
        assert_eq!(state.input().active_name().as_deref(), Some("Mock Monitor"));
    }
    let response = app.oneshot(buffer_request(1)).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(on_air_core::status_for(&state).capture_buffer_ms, Some(0));
}

#[tokio::test]
async fn a_benchmark_never_changes_the_live_buffer_or_publishes_synthetic_audio() {
    let state = CoreState::new_mock().await;
    let mut live_audio = state.audio_tx.subscribe();
    let app = on_air_core::build_router(state.clone());
    app.clone().oneshot(buffer_request(100)).await.unwrap();
    let response = app
        .oneshot(
            Request::post("/api/audio/benchmark")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(on_air_core::status_for(&state).capture_buffer_ms, Some(100));
    assert!(matches!(
        live_audio.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn capture_buffer_survives_core_restart() {
    let folder = std::env::temp_dir().join(format!("on-air-buffer-test-{}", std::process::id()));
    let path = folder.join("settings.json");
    let state = CoreState::new_persistent(&path);
    let response = on_air_core::build_router(state.clone())
        .oneshot(buffer_request(200))
        .await
        .unwrap();
    // Production routers require connection metadata for loopback trust.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let request = buffer_request(200);
    let mut request = request;
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = on_air_core::build_router(state.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    state.shutdown().await;
    let restored = CoreState::new_persistent(&path);
    assert_eq!(
        on_air_core::status_for(&restored).capture_buffer_ms,
        Some(200)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

fn quality_request(quality: &str) -> Request<Body> {
    Request::put("/api/audio/quality")
        .header("content-type", "application/json")
        .body(Body::from(format!("{{\"quality\":\"{quality}\"}}")))
        .unwrap()
}

fn tuning_request() -> Request<Body> {
    Request::get("/api/audio/tuning").body(Body::empty()).unwrap()
}

async fn tuning_json(app: &axum::Router, request: Request<Body>) -> serde_json::Value {
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn the_quality_override_is_recorded_reported_and_survives_checks() {
    let state = CoreState::new_mock().await;
    let app = on_air_core::build_router(state.clone());
    let before = tuning_json(&app, tuning_request()).await;
    assert_eq!(before["tuned"], serde_json::json!(false));
    assert_eq!(before["quality"], serde_json::json!("economy"));
    assert_eq!(before["quality_override"], serde_json::json!(null));
    assert_eq!(
        before["profiles"],
        serde_json::json!(["economy", "balanced", "high"])
    );

    let response = app.clone().oneshot(quality_request("high")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let after = tuning_json(&app, tuning_request()).await;
    assert_eq!(after["quality"], serde_json::json!("high"));
    assert_eq!(after["quality_override"], serde_json::json!("high"));
}

#[tokio::test]
async fn a_manual_check_marks_the_machine_as_tuned_without_changing_settings() {
    let state = CoreState::new_mock().await;
    state.input().activate("Mock Monitor").await.unwrap();
    let app = on_air_core::build_router(state.clone());
    let response = app
        .clone()
        .oneshot(buffer_request(100))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .clone()
        .oneshot(quality_request("high"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .clone()
        .oneshot(
            Request::post("/api/audio/benchmark")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let tuning = tuning_json(&app, tuning_request()).await;
    assert_eq!(tuning["tuned"], serde_json::json!(true));
    assert!(!tuning["recommended_quality"].is_null());
    // Non-destructive: the manual button applies nothing.
    assert_eq!(tuning["capture_buffer_ms"], serde_json::json!(100));
    assert_eq!(tuning["quality"], serde_json::json!("high"));
    assert_eq!(tuning["automatic"], serde_json::json!(null));
}
