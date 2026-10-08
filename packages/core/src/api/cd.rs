use crate::api::error::{ApiError, JsonBody};
use crate::api::inputs;
use crate::auth::Paired;
use crate::cd::{eject_drive, probe_audio_cd, CdMediaEvent, CdStatus, GeneratedCd, AUDIO_CD_INPUT};
use crate::state::CoreState;
use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

const WATCH_INTERVAL: Duration = Duration::from_secs(1);

pub async fn get_cd(Paired: Paired, State(state): State<CoreState>) -> Json<CdStatus> {
    Json(state.cd.status())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdAction {
    Play,
    Pause,
    Next,
    Prev,
    Seek,
    Goto,
    Eject,
}

#[derive(Deserialize)]
pub struct CdControlRequest {
    pub action: CdAction,
    pub position_ms: Option<u64>,
    pub track: Option<u8>,
}

pub async fn control_cd(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<CdControlRequest>,
) -> Result<Json<CdStatus>, ApiError> {
    if req.action == CdAction::Eject {
        if !state.mock {
            // The eject ioctl blocks while the tray moves; keep it off the
            // async workers.
            let _ = tokio::task::spawn_blocking(eject_drive).await;
        }
        let _ = state.cd.eject();
        on_ejected(&state).await;
        return Ok(Json(state.cd.status()));
    }
    if !state.cd.status().present {
        return Err(ApiError::conflict("no audio compact disc"));
    }
    match req.action {
        CdAction::Play => {
            ensure_cd_capture(&state).await;
            state.cd.play();
        }
        CdAction::Pause => state.cd.pause(),
        CdAction::Next => {
            let _ = state.cd.next();
        }
        CdAction::Prev => {
            let _ = state.cd.prev();
        }
        CdAction::Seek => {
            let Some(position_ms) = req.position_ms else {
                return Err(ApiError::validation("seek needs position_ms"));
            };
            state.cd.seek_ms(position_ms);
        }
        CdAction::Goto => {
            let Some(track) = req.track else {
                return Err(ApiError::validation("goto needs track"));
            };
            state.cd.goto_track(track);
        }
        CdAction::Eject => unreachable!("handled above"),
    }
    Ok(Json(state.cd.status()))
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

/// `POST /api/mock/cd`, routed only in mock mode: insert or eject a disc so
/// UI and API tests can exercise autoplay.
pub async fn simulate_cd(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<CdSimulateRequest>,
) -> Result<Json<CdStatus>, ApiError> {
    if !state.mock {
        return Err(ApiError::not_found("no such route"));
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
            return Err(ApiError::validation("audio cd needs tracks"));
        }
        let named: Vec<(String, u64)> = tracks
            .iter()
            .map(|track| {
                (
                    track.title.clone().unwrap_or_default(),
                    track.duration_ms.unwrap_or(180_000),
                )
            })
            .collect();
        let medium = GeneratedCd::from_tracks(req.album, &named);
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
    Ok(Json(state.cd.status()))
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
    if let Err(error) = inputs::activate_input_named(state, AUDIO_CD_INPUT).await {
        eprintln!("cd autoplay could not take the input: {error}");
        return;
    }
    state.cd.play();
}

pub async fn on_ejected(state: &CoreState) {
    let is_cd = state.active_input.lock().as_deref() == Some(AUDIO_CD_INPUT);
    if !is_cd {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *state.active_input.lock() = None;
    state.clear_saved_input();
}

async fn ensure_cd_capture(state: &CoreState) {
    let already = state.active_input.lock().as_deref() == Some(AUDIO_CD_INPUT);
    if already {
        return;
    }
    let _configuration = state.config_lock.lock().await;
    if let Err(error) = inputs::activate_input_named(state, AUDIO_CD_INPUT).await {
        eprintln!("cd play could not take the input: {error}");
    }
}

/// Poll the optical drive. Only started for a non-mock state.
pub fn spawn_watch(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut empty_probes = 0u8;
        loop {
            let found = tokio::task::spawn_blocking(probe_audio_cd)
                .await
                .ok()
                .flatten();
            if found.is_none() {
                empty_probes = empty_probes.saturating_add(1);
                if empty_probes < 3 {
                    tokio::time::sleep(WATCH_INTERVAL).await;
                    continue;
                }
            } else {
                empty_probes = 0;
            }
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
