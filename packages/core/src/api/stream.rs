use crate::dsp::bridge::RateBridge;
use crate::state::CoreState;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::sync::atomic::Ordering;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

const MAX_STREAM_CLIENTS: usize = 4;

struct StreamClientGuard(CoreState);

impl Drop for StreamClientGuard {
    fn drop(&mut self) {
        self.0.stream_clients.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Streaming WAV header: 16-bit PCM mono, unknown/very large data size so
/// Sonos can treat GET /stream/audio.wav as a live HTTP radio source.
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

pub fn sonos_radio_is_live(state: &CoreState) -> bool {
    state
        .active_output
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|o| o.transport == "sonos" || o.transport == "airplay")
}

pub async fn stream_audio(State(state): State<CoreState>) -> Response {
    if !state.service_enabled.load(Ordering::Acquire) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !sonos_radio_is_live(&state) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if state.stream_clients.fetch_add(1, Ordering::AcqRel) >= MAX_STREAM_CLIENTS {
        state.stream_clients.fetch_sub(1, Ordering::AcqRel);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "too many audio stream clients",
        )
            .into_response();
    }
    let client_guard = StreamClientGuard(state.clone());
    let generation = state.stream_generation.load(Ordering::Acquire);
    let rx = state.audio_tx.subscribe();
    let pipeline_hz = *state.target_sample_rate_hz.lock().unwrap();
    let output_hz = *state.output_sample_rate_hz.lock().unwrap();
    let header = wav_header(output_hz);
    let pcm = BroadcastStream::new(rx)
        .take_while({
            let stream_generation = state.stream_generation.clone();
            let service_enabled = state.service_enabled.clone();
            move |_| {
                service_enabled.load(Ordering::Acquire)
                    && stream_generation.load(Ordering::Acquire) == generation
            }
        })
        .filter_map({
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
    let body = Body::from_stream(tokio_stream::once(Ok(header)).chain(pcm));

    (
        [
            (header::CONTENT_TYPE, "audio/wav".to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}
