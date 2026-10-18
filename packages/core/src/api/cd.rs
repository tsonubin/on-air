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

pub use on_air_api::{CdAction, CdControlRequest};

pub async fn control_cd(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<CdControlRequest>,
) -> Result<Json<CdStatus>, ApiError> {
    let status = crate::cd::control(&state, &req)
        .await
        .map_err(|error| match error {
            crate::cd::ControlError::NoDisc => ApiError::not_ready(error.to_string()),
            crate::cd::ControlError::SeekNeedsPosition
            | crate::cd::ControlError::GotoNeedsTrack => ApiError::validation(error.to_string()),
        })?;
    Ok(Json(status))
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
