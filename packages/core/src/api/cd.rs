use crate::api::inputs;
use crate::auth::Paired;
use crate::cd::{probe_audio_cd, CdMediaEvent, CdStatus, GeneratedCd, AUDIO_CD_INPUT};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

const WATCH_INTERVAL: Duration = Duration::from_secs(1);

pub async fn get_cd(Paired: Paired, State(state): State<CoreState>) -> Json<CdStatus> {
    Json(state.cd.status())
}

#[derive(Deserialize)]
pub struct CdControlRequest {
    pub action: String,
}

pub async fn control_cd(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<CdControlRequest>,
) -> Response {
    if !state.cd.status().present {
        return (StatusCode::CONFLICT, "no audio compact disc").into_response();
    }
    match req.action.as_str() {
        "play" => {
            ensure_cd_capture(&state).await;
            state.cd.play();
        }
        "pause" => state.cd.pause(),
        "next" => {
            let _ = state.cd.next();
        }
        "prev" => {
            let _ = state.cd.prev();
        }
        _ => return (StatusCode::BAD_REQUEST, "unknown cd action").into_response(),
    }
    Json(state.cd.status()).into_response()
}

#[derive(Deserialize)]
pub struct CdSimulateTrack {
    pub title: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Deserialize)]
pub struct CdSimulateRequest {
    pub present: bool,
    pub album: Option<String>,
    pub tracks: Option<Vec<CdSimulateTrack>>,
}

/// Mock-only: insert or eject a disc so UI and API tests can exercise autoplay.
pub async fn simulate_cd(
    Paired: Paired,
    State(state): State<CoreState>,
    Json(req): Json<CdSimulateRequest>,
) -> Response {
    if !state.mock {
        return StatusCode::NOT_FOUND.into_response();
    }
    if req.present {
        let tracks = req.tracks.unwrap_or_else(|| {
            vec![
                CdSimulateTrack {
                    title: Some("Track 1".into()),
                    duration_ms: Some(180_000),
                },
                CdSimulateTrack {
                    title: Some("Track 2".into()),
                    duration_ms: Some(180_000),
                },
                CdSimulateTrack {
                    title: Some("Track 3".into()),
                    duration_ms: Some(180_000),
                },
            ]
        });
        if tracks.is_empty() {
            return (StatusCode::BAD_REQUEST, "audio cd needs tracks").into_response();
        }
        let titles: Vec<String> = tracks
            .iter()
            .map(|track| track.title.clone().unwrap_or_default())
            .collect();
        let duration_ms = tracks
            .first()
            .and_then(|track| track.duration_ms)
            .unwrap_or(180_000);
        let medium = GeneratedCd::from_titles(req.album, &titles, duration_ms);
        let event = state.cd.set_medium(Some(Arc::new(medium)));
        if event == CdMediaEvent::Inserted {
            autoplay(&state).await;
        }
    } else {
        let event = state.cd.set_medium(None);
        if event == CdMediaEvent::Ejected {
            on_ejected(&state).await;
        }
    }
    Json(state.cd.status()).into_response()
}

pub async fn autoplay(state: &CoreState) {
    if !state
        .service_enabled
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return;
    }
    if !state.cd.status().present {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if let Err((_, error)) = inputs::activate_input_named(state, AUDIO_CD_INPUT).await {
        eprintln!("cd autoplay could not take the input: {error}");
        return;
    }
    state.cd.play();
}

pub async fn on_ejected(state: &CoreState) {
    let is_cd = state.active_input.lock().unwrap().as_deref() == Some(AUDIO_CD_INPUT);
    if !is_cd {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *state.active_input.lock().unwrap() = None;
}

async fn ensure_cd_capture(state: &CoreState) {
    let already = state.active_input.lock().unwrap().as_deref() == Some(AUDIO_CD_INPUT);
    if already {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if let Err((_, error)) = inputs::activate_input_named(state, AUDIO_CD_INPUT).await {
        eprintln!("cd play could not take the input: {error}");
    }
}

pub fn spawn_watch(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if state.mock {
                tokio::time::sleep(WATCH_INTERVAL).await;
                continue;
            }
            let found = tokio::task::spawn_blocking(probe_audio_cd)
                .await
                .ok()
                .flatten();
            let event = state.cd.set_medium(found);
            match event {
                CdMediaEvent::Inserted => autoplay(&state).await,
                CdMediaEvent::Ejected => on_ejected(&state).await,
                CdMediaEvent::Unchanged => {}
            }
            tokio::time::sleep(WATCH_INTERVAL).await;
        }
    })
}
