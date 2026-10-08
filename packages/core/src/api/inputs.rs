use crate::api::error::{ApiError, JsonBody};
use crate::auth::Paired;
use crate::cd::{AUDIO_CD_INPUT, CD_SAMPLE_RATE_HZ};
use crate::pipeline::{self, capture};
use crate::state::CoreState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

const RING_BUFFER_DURATION_SECONDS: usize = 2;

/// Virtual input name a client may POST to `/api/inputs/active` to mean
/// "whatever this OS exposes as the system-audio loopback". The core resolves
/// it to the preferred cpal loopback device, falling back to a Pulse monitor
/// source on Linux. Kept in sync with the desktop and mobile clients.
pub const LOOPBACK_INPUT: &str = "__loopback__";

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("{0}")]
    NotFound(&'static str),
    #[error("no audio compact disc")]
    NoDisc,
    #[error("{0}")]
    Capture(String),
}

#[derive(Serialize)]
pub struct InputsResponse {
    pub inputs: Vec<String>,
}

fn with_cd_input(state: &CoreState, mut inputs: Vec<String>) -> Vec<String> {
    if state.cd.status().present && !inputs.iter().any(|name| name == AUDIO_CD_INPUT) {
        inputs.insert(0, AUDIO_CD_INPUT.to_string());
    }
    inputs
}

pub async fn list_inputs(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Result<Json<InputsResponse>, ApiError> {
    if state.mock {
        return Ok(Json(InputsResponse {
            inputs: with_cd_input(&state, state.mock_inputs.lock().clone()),
        }));
    }
    // CPAL enumeration and the Linux `pactl` fallback are synchronous and may
    // take up to two seconds. Never pin an async request worker while probing.
    let devices = tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        capture::list_input_devices(&host)
    })
    .await
    .map_err(|error| ApiError::internal(error.to_string()))?
    .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(InputsResponse {
        inputs: with_cd_input(&state, devices.into_iter().map(|d| d.name).collect()),
    }))
}

#[derive(Serialize)]
pub struct ActiveInputResponse {
    pub name: Option<String>,
    pub backend: &'static str,
}

pub async fn get_active_input(
    Paired: Paired,
    State(state): State<CoreState>,
) -> Json<ActiveInputResponse> {
    Json(ActiveInputResponse {
        name: state.active_input.lock().clone(),
        backend: capture::loopback_backend(),
    })
}

#[derive(Deserialize)]
pub struct ActivateInputRequest {
    pub name: String,
}

pub async fn activate_input(
    Paired: Paired,
    State(state): State<CoreState>,
    JsonBody(req): JsonBody<ActivateInputRequest>,
) -> Result<StatusCode, ApiError> {
    let _configuration = state.config_lock.lock().await;
    activate_input_named(&state, &req.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Activate an input while the caller holds `CoreState::config_lock`.
pub(crate) async fn activate_input_named(
    state: &CoreState,
    requested_name: &str,
) -> Result<(), InputError> {
    if state.mock {
        let cd_ok = requested_name == AUDIO_CD_INPUT && state.cd.status().present;
        let known = cd_ok
            || state
                .mock_inputs
                .lock()
                .iter()
                .any(|name| name == requested_name);
        if !known {
            return Err(InputError::NotFound("input device not found"));
        }
        let name = requested_name.to_string();
        *state.active_input.lock() = Some(name.clone());
        state.remember_input(name);
        return Ok(());
    }

    let source_name = if requested_name == LOOPBACK_INPUT {
        let resolved = tokio::task::spawn_blocking(|| {
            let host = cpal::default_host();
            capture::find_preferred_loopback(&host).map(|device| {
                device
                    .map(|device| device.to_string())
                    .or_else(capture::preferred_pulse_monitor)
            })
        })
        .await;
        match resolved {
            Ok(Ok(Some(name))) => name,
            Ok(Ok(None)) => return Err(InputError::NotFound("no loopback capture device")),
            Ok(Err(error)) => return Err(InputError::Capture(error.to_string())),
            Err(error) => return Err(InputError::Capture(error.to_string())),
        }
    } else {
        requested_name.to_string()
    };

    // Start the replacement first so a failed start leaves the current input
    // running. The CD deck's producer carries a generation token, so the old
    // handle's cleanup cannot detach the producer the new handle attached.
    let new_handle = start_named_capture(state, source_name.clone()).await?;

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        let _ = tokio::task::spawn_blocking(move || old.stop()).await;
    }
    *guard = Some(new_handle);
    *state.active_input.lock() = Some(source_name.clone());
    state.remember_input(source_name);

    Ok(())
}

/// Rebuild the active capture pipeline after a sample-rate change. The
/// configuration lock must be held by the caller.
pub(crate) async fn restart_active_capture(state: &CoreState) -> Result<(), InputError> {
    if state.mock {
        return Ok(());
    }
    let Some(name) = state.active_input.lock().clone() else {
        return Ok(());
    };

    let mut guard = state.capture.lock().await;
    if let Some(old) = guard.take() {
        tokio::task::spawn_blocking(move || old.stop())
            .await
            .map_err(|e| InputError::Capture(e.to_string()))?;
    }
    let handle = start_named_capture(state, name).await?;
    *guard = Some(handle);
    Ok(())
}

async fn start_named_capture(
    state: &CoreState,
    name: String,
) -> Result<pipeline::CaptureHandle, InputError> {
    let target_rate = *state.target_sample_rate_hz.lock();
    let ring_capacity = (target_rate as usize).saturating_mul(RING_BUFFER_DURATION_SECONDS);
    let (producer, consumer) = pipeline::new_ring_buffer(ring_capacity);
    let eq = state.eq_gains_db.clone();
    let audio_tx = state.audio_tx.clone();
    let ws_tx = state.ws_tx.clone();
    let label = name.clone();

    if name == AUDIO_CD_INPUT {
        if !state.cd.status().present {
            return Err(InputError::NoDisc);
        }
        let generation = state.cd.attach_producer(producer);
        let deck = state.cd.clone();
        *state.input_supported_hz.lock() = crate::dsp::rates::INPUT_RATES_HZ.to_vec();
        let processing = pipeline::spawn_processing_task(
            consumer,
            CD_SAMPLE_RATE_HZ,
            target_rate,
            eq,
            audio_tx,
            ws_tx,
        );
        return Ok(pipeline::CaptureHandle::with_cleanup(
            processing,
            label,
            Box::new(move || deck.detach_producer(generation)),
        ));
    }

    if capture::is_pulse_monitor_name(&name) {
        let src = name.clone();
        let started = tokio::task::spawn_blocking(move || {
            capture::start_pulse_monitor(&src, producer, Some(target_rate))
        })
        .await
        .map_err(|e| InputError::Capture(e.to_string()))?;
        let (pulse, input_rate) = started.map_err(InputError::Capture)?;
        *state.input_supported_hz.lock() = crate::dsp::rates::INPUT_RATES_HZ.to_vec();
        let processing =
            pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
        return Ok(pipeline::CaptureHandle::pulse(pulse, processing, label));
    }

    let started = tokio::task::spawn_blocking(move || {
        let host = cpal::default_host();
        let Some(device) = capture::find_input_device(&host, &name).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let supported_ranges = capture::supported_input_rate_ranges(&device);
        capture::start_capture_at(&device, producer, Some(target_rate))
            .map(|(stream, input_rate)| Some((stream, input_rate, supported_ranges)))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| InputError::Capture(e.to_string()))?;
    let Some((stream, input_rate, supported_ranges)) = started.map_err(InputError::Capture)? else {
        return Err(InputError::NotFound("input device not found"));
    };
    *state.input_supported_hz.lock() =
        crate::dsp::rates::intersect_catalog(&supported_ranges, crate::dsp::rates::INPUT_RATES_HZ);
    let processing =
        pipeline::spawn_processing_task(consumer, input_rate, target_rate, eq, audio_tx, ws_tx);
    Ok(pipeline::CaptureHandle::cpal(stream, processing, label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cd::GeneratedCd;
    use std::sync::Arc;
    use std::time::Duration;

    /// Audit finding #2: re-activating "Audio CD" while it is already the
    /// input used to detach the producer the new handle had just attached,
    /// because the old handle's cleanup ran after the new attach.
    #[tokio::test]
    async fn reactivating_the_cd_input_keeps_the_fresh_producer_attached() {
        let state = CoreState::new();
        let medium = GeneratedCd::from_titles(Some("Album".into()), &["A".to_string()], 60_000);
        state.cd.set_medium(Some(Arc::new(medium)));

        activate_input_named(&state, AUDIO_CD_INPUT).await.unwrap();
        assert!(state.cd.has_producer());
        activate_input_named(&state, AUDIO_CD_INPUT).await.unwrap();
        assert!(
            state.cd.has_producer(),
            "second activation must leave the deck with a producer"
        );
        assert_eq!(
            state
                .capture
                .lock()
                .await
                .as_ref()
                .map(|handle| handle.device_name.clone())
                .as_deref(),
            Some(AUDIO_CD_INPUT)
        );

        // The live handle must still move PCM from the deck to the bus.
        let mut rx = state.audio_tx.subscribe();
        state.cd.play();
        let chunk = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("pcm flowed from the re-activated cd input")
            .unwrap();
        assert!(!chunk.is_empty());

        state.shutdown().await;
    }
}
