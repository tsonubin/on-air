use crate::api::error::ApiError;
use crate::dsp::bridge::RateBridge;
use crate::state::CoreState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::sync::atomic::Ordering;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

const MAX_STREAM_CLIENTS: usize = 4;
const SILENCE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(40);

struct StreamClientGuard(CoreState);

impl Drop for StreamClientGuard {
    fn drop(&mut self) {
        self.0.stream_clients.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Streaming WAV header: 16-bit PCM mono, unknown/very large data size so
/// Sonos can treat the radio GET as a live HTTP source.
fn wav_header(sample_rate: u32) -> Bytes {
    let channels = 1u16;
    let bits = 16u16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits) / 8;
    let block_align = channels * bits / 8;
    let data_size = u32::MAX - 36;
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&data_size.wrapping_add(36).to_le_bytes());
    h.extend_from_slice(b"WAVE");
    h.extend_from_slice(b"fmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&channels.to_le_bytes());
    h.extend_from_slice(&sample_rate.to_le_bytes());
    h.extend_from_slice(&byte_rate.to_le_bytes());
    h.extend_from_slice(&block_align.to_le_bytes());
    h.extend_from_slice(&bits.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_size.to_le_bytes());
    Bytes::from(h)
}

fn silence_chunk(sample_rate: u32) -> Bytes {
    let frames =
        ((u64::from(sample_rate) * SILENCE_INTERVAL.as_millis() as u64) / 1_000).max(1) as usize;
    Bytes::from(vec![0; frames.saturating_mul(2)])
}

/// True while a Sonos or AirPlay output is live and `nonce` is the path
/// segment issued for that activation. Bluetooth plays through the OS and
/// never exposes the radio.
pub fn radio_stream_is_live(state: &CoreState, nonce: &str) -> bool {
    let pulls_radio = state
        .active_output
        .lock()
        .as_ref()
        .is_some_and(|o| o.transport == "sonos" || o.transport == "airplay");
    pulls_radio && state.stream_nonce.lock().as_deref() == Some(nonce)
}

pub async fn stream_audio(
    State(state): State<CoreState>,
    Path(nonce): Path<String>,
) -> Result<Response, ApiError> {
    if !state.service_enabled.load(Ordering::Acquire) {
        return Err(ApiError::service_paused());
    }
    if !radio_stream_is_live(&state, &nonce) {
        return Err(ApiError::not_found("no live audio stream at this address"));
    }
    if state.stream_clients.fetch_add(1, Ordering::AcqRel) >= MAX_STREAM_CLIENTS {
        state.stream_clients.fetch_sub(1, Ordering::AcqRel);
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "stream_busy",
            "too many audio stream clients",
        ));
    }
    let client_guard = StreamClientGuard(state.clone());
    let rx = state.audio_tx.subscribe();
    let pipeline_hz = *state.target_sample_rate_hz.lock();
    let output_hz = *state.output_sample_rate_hz.lock();
    let header = wav_header(output_hz);
    let pcm = BroadcastStream::new(rx).filter_map({
        let mut bridge = RateBridge::new(pipeline_hz, output_hz, 1);
        let _client_guard = client_guard;
        move |item| {
            let _ = &_client_guard;
            let Ok(chunk) = item else {
                return None;
            };
            if pipeline_hz == output_hz {
                return Some(Ok::<Bytes, std::convert::Infallible>(chunk));
            }
            let resampled = bridge.process_l16_mono_to_l16(&chunk);
            if resampled.is_empty() {
                return None;
            }
            Some(Ok(Bytes::from(resampled)))
        }
    });
    let silence = silence_chunk(output_hz);
    let mut silence_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + SILENCE_INTERVAL,
        SILENCE_INTERVAL,
    );
    silence_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let stream_progress = state.stream_progress.clone();
    let pcm = pcm
        .timeout_repeating(silence_interval)
        .map(move |item| {
            stream_progress.fetch_add(1, Ordering::Relaxed);
            match item {
                Ok(pcm) => pcm,
                Err(_) => Ok(silence.clone()),
            }
        })
        .take_while({
            let stream_nonce = state.stream_nonce.clone();
            let service_enabled = state.service_enabled.clone();
            move |_| {
                service_enabled.load(Ordering::Acquire)
                    && stream_nonce.lock().as_deref() == Some(nonce.as_str())
            }
        });
    let body = Body::from_stream(tokio_stream::once(Ok(header)).chain(pcm));

    Ok((
        [
            (header::CONTENT_TYPE, "audio/wav".to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response())
}
