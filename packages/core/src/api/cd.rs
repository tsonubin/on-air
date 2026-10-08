use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::cd::autoplay;
use crate::cd::{CdStatus, GeneratedCd};
use crate::state::CoreState;
use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

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
        autoplay::eject(&state).await;
        return Ok(Json(state.cd.status()));
    }
    if !state.cd.status().present {
        return Err(ApiError::not_ready("no audio compact disc"));
    }
    match req.action {
        CdAction::Play => {
            autoplay::ensure_capture(&state).await;
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
        autoplay::on_media_event(&state, event).await;
    } else {
        let event = state.cd.set_medium(None);
        autoplay::on_media_event(&state, event).await;
    }
    Ok(Json(state.cd.status()))
}
