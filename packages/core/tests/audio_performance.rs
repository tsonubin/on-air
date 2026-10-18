use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use on_air_core::{
    audio_benchmark::{recommend_quality, recommendation, run},
    dsp::DspQuality,
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

/// Measured worst-case per-chunk bridge cost at each profile, in
/// `DspQuality::ALL` order — the shape `recommend_quality` consumes. These
/// numbers are synthetic on purpose: the rule must move the right way for
/// any honest cost vector, and the benchmark measures the real one on the
/// machine that runs it.
const COSTS: [f64; 3] = [0.1, 0.4, 0.8];

#[test]
fn quality_recommendation_degrades_as_the_measured_budget_worsens() {
    // 1024 frames at 44.1 kHz is a 23.2 ms chunk. Growing the measured
    // worst round trip must walk the ladder down: High, then Balanced, then
    // Economy, and never back up.
    let chunk_ms = 1024.0 / 44_100.0 * 1000.0;
    let recommended =
        |worst_ms: f64| recommend_quality(worst_ms, chunk_ms, DspQuality::Economy, &COSTS);
    assert_eq!(recommended(1.0), DspQuality::High);
    assert_eq!(recommended(7.2), DspQuality::Balanced);
    assert_eq!(recommended(7.6), DspQuality::Economy);
    assert_eq!(recommended(200.0), DspQuality::Economy);
    // Same ordering for a shrinking real-time budget (a higher sample
    // rate means a shorter chunk period).
    assert_eq!(
        recommend_quality(0.5, 3.0, DspQuality::Economy, &COSTS),
        DspQuality::Balanced
    );
    assert_eq!(
        recommend_quality(0.9, 3.0, DspQuality::Economy, &COSTS),
        DspQuality::Economy
    );
}

#[test]
fn quality_recommendation_subtracts_the_measured_profile_it_ran_at() {
    // The same observed round trip recommends a higher profile when it was
    // measured at a costlier one: the swappable part is subtracted before
    // each candidate's measured cost is added back.
    let chunk_ms = 1024.0 / 44_100.0 * 1000.0;
    assert_eq!(
        recommend_quality(7.6, chunk_ms, DspQuality::Economy, &COSTS),
        DspQuality::Economy
    );
    assert_eq!(
        recommend_quality(7.6, chunk_ms, DspQuality::High, &COSTS),
        DspQuality::High
    );
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
    let report = run(
        44_100,
        48_000,
        [2.0, 0.0, 0.0, 0.0, -2.0],
        8,
        DspQuality::Economy,
    )
    .unwrap();
    assert_eq!(report.input_hz, 44_100);
    assert_eq!(report.output_hz, 48_000);
    assert!(report.duration_ms > 0);
    assert!(report.pipeline_p95_ms.is_finite());
    assert!(report.scheduler_p95_ms >= 0.0);
    assert!([50, 100, 200, 400].contains(&report.recommended_buffer_ms));
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

async fn quality_response(app: axum::Router) -> serde_json::Value {
    let response = app
        .oneshot(
            Request::get("/api/audio/quality")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn quality_profile_round_trips_over_the_api_and_rejects_unknown_values() {
    let state = CoreState::new_mock().await;
    let app = on_air_core::build_router(state.clone());
    // A fresh install is economy — the historical cost profile — and with
    // no benchmark yet the recommendation is the profile in use.
    assert_eq!(
        quality_response(app.clone()).await,
        serde_json::json!({
            "quality": "economy",
            "recommended": "economy",
            "profiles": ["economy", "balanced", "high"],
        })
    );
    for quality in ["balanced", "high", "economy"] {
        let response = app.clone().oneshot(quality_request(quality)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(quality_response(app.clone()).await["quality"], quality);
    }
    let response = app.oneshot(quality_request("ultra")).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error["code"], "invalid_request");
    assert_eq!(state.dsp_quality(), DspQuality::Economy);
}

#[tokio::test]
async fn the_benchmark_recommends_a_quality_profile_that_is_remembered() {
    let state = CoreState::new_mock().await;
    let app = on_air_core::build_router(state.clone());
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let recommended = report["recommended_quality"].as_str().unwrap().to_string();
    assert!(
        ["economy", "balanced", "high"].contains(&recommended.as_str()),
        "the benchmark recommends a known profile, got {recommended}"
    );
    // The verdict is remembered as the `recommended` field until another
    // run replaces it, and it is what GET reports.
    let quality = quality_response(app).await;
    assert_eq!(quality["recommended"], recommended);
    assert_eq!(
        state.dsp_quality(),
        DspQuality::Economy,
        "a benchmark never changes the chosen profile"
    );
}

#[tokio::test]
async fn the_quality_profile_survives_core_restart() {
    let folder = std::env::temp_dir().join(format!("on-air-quality-test-{}", std::process::id()));
    let path = folder.join("settings.json");
    let state = CoreState::new_persistent(&path);
    let mut request = quality_request("high");
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
    assert_eq!(restored.dsp_quality(), DspQuality::High);
    assert_eq!(
        restored.audio_quality().recommended,
        DspQuality::High,
        "without a benchmark the recommendation follows the chosen profile"
    );
    std::fs::remove_dir_all(folder).unwrap();
}
