use crate::api::auth::Paired;
use crate::api::error::{ApiError, JsonBody};
use crate::session;
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

pub use on_air_api::{
    ActivateOutputRequest, ActiveOutputResponse, ActiveOutputView, ChannelMode,
    OutputVolumeResponse, OutputsResponse, SetChannelModeRequest, SetVolumeRequest,
};

pub async fn list_outputs(Paired: Paired, State(state): State<CoreState>) -> Json<OutputsResponse> {
    Json(OutputsResponse {
        outputs: session::list(&state).await,
    })
}

pub async fn get_active_output(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<ActiveOutputResponse> {
    let snapshot = state.output().snapshot();
    Json(ActiveOutputResponse {
        active: snapshot.identity.map(|output| ActiveOutputView {
            output,
            state: Some(snapshot.phase),
        }),
    })
}

pub async fn activate_output(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<ActivateOutputRequest>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    // The service may have been turned off while this request waited.
    if !state
        .service_enabled
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(ApiError::service_paused());
    }
    session::activate(&state, &req.transport, &req.device_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/outputs/active`: stop casting and forget the saved output so
/// the next launch does not resume it.
pub async fn deactivate_output(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    state.output().deactivate().await?;
    state.clear_saved_output();
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_output_volume(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<OutputVolumeResponse> {
    Json(OutputVolumeResponse {
        volume: state.output_volume(),
    })
}

pub async fn set_output_volume(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<SetVolumeRequest>,
) -> Result<StatusCode, ApiError> {
    if req.volume > 100 {
        return Err(ApiError::validation("volume must be between 0 and 100"));
    }
    session::set_volume(&state, req.volume).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/outputs/channel-mode`: how one output fits the pipeline stereo
/// signal to its destination (roadmap A4). `stereo` on a destination the
/// core knows plays mono is refused with an explanation of what to do,
/// never silently collapsed; the mode is saved and takes effect the next
/// time that output opens (its next activation or stream connection).
pub async fn set_channel_mode(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<SetChannelModeRequest>,
) -> Result<StatusCode, ApiError> {
    if req.mode == ChannelMode::Stereo {
        let known = session::list(&state)
            .await
            .into_iter()
            .find(|output| output.transport == req.transport && output.id == req.device_id);
        if let Some(output) = known {
            if output.channels == Some(1) {
                return Err(ApiError::validation(
                    crate::dsp::channel::ChannelModeError::StereoOnMono { name: output.name }
                        .message(),
                ));
            }
        }
    }
    state.remember_channel_mode(&req.transport, &req.device_id, req.mode);
    Ok(StatusCode::NO_CONTENT)
}
