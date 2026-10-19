use crate::api::error::ApiError;
use crate::dsp::bridge::RateBridge;
use crate::dsp::channel::{ChannelMode, ChannelPlan, Destination, SinkConstruction};
use crate::dsp::PIPELINE_CHANNELS;
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

/// Streaming WAV header: 16-bit PCM, unknown/very large data size so Sonos
/// can treat the radio GET as a live HTTP source. The channel count is the
/// plan's, never a caller-spelled number (roadmap A6).
fn wav_header(sample_rate: u32, plan: &ChannelPlan) -> Bytes {
    let channels = plan.channels() as u16;
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

fn silence_chunk(sample_rate: u32, plan: &ChannelPlan) -> Bytes {
    let frames =
        ((u64::from(sample_rate) * SILENCE_INTERVAL.as_millis() as u64) / 1_000).max(1) as usize;
    Bytes::from(vec![0; frames.saturating_mul(plan.channels() * 2)])
}

/// The radio WAV's channel fit: the construction site Sonos and AirPlay
/// pull (walked by `tests/channel_construction.rs`, roadmap A6). The
/// surface is stereo by construction, so every mode fits; `auto` keeps the
/// historical stereo stream byte for byte (roadmap A4).
pub fn radio_plan(mode: ChannelMode) -> ChannelPlan {
    SinkConstruction::RadioWav
        .plan(mode, Destination::stereo("the radio stream"))
        .expect("the radio surface is stereo, so every mode but stereo-on-mono fits")
}

/// True while a Sonos or AirPlay output owns `nonce`, the path segment
/// issued for that activation. Bluetooth plays through the OS and never
/// exposes the radio.
pub fn radio_stream_is_live(state: &CoreState, nonce: &str) -> bool {
    state.output().snapshot().serves_radio(nonce)
}

pub async fn stream_audio(
    State(state): State<CoreState>,
    Path(nonce): Path<String>,
) -> Result<Response, ApiError> {
    let state = if radio_stream_is_live(&state, &nonce) {
        state
    } else {
        state.patchbay.stream_state(&nonce).await.unwrap_or(state)
    };
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
    // The radio serves Sonos and AirPlay: a stereo WAV surface whose
    // channel mode belongs to the active output (mono becomes a genuine
    // mono WAV; `auto` is byte-identical to the historical stereo stream).
    let plan = {
        let identity = state.output().snapshot().identity;
        let mode = identity
            .as_ref()
            .map(|output| state.channel_mode(&output.transport, &output.device_id))
            .unwrap_or_default();
        radio_plan(mode)
    };
    let header = wav_header(output_hz, &plan);
    let stream_channels = plan.channels();
    let pcm = BroadcastStream::new(rx).filter_map({
        let mut bridge = RateBridge::new(pipeline_hz, output_hz, &plan);
        let _client_guard = client_guard;
        move |item| {
            let _ = &_client_guard;
            let Ok(chunk) = item else {
                return None;
            };
            if pipeline_hz == output_hz && stream_channels == PIPELINE_CHANNELS {
                return Some(Ok::<Bytes, std::convert::Infallible>(chunk));
            }
            let resampled = bridge.process_l16_to_l16(&chunk);
            if resampled.is_empty() {
                return None;
            }
            Some(Ok(Bytes::from(resampled)))
        }
    });
    let silence = silence_chunk(output_hz, &plan);
    let mut silence_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + SILENCE_INTERVAL,
        SILENCE_INTERVAL,
    );
    silence_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let stream_progress = state.stream_progress.clone();
    let capture_buffer_ms = state.capture_buffer_ms.clone();
    let mut last_pcm = tokio::time::Instant::now();
    let pcm = pcm
        .timeout_repeating(silence_interval)
        .filter_map(move |item| {
            let pcm = match item {
                Ok(pcm) => {
                    last_pcm = tokio::time::Instant::now();
                    pcm
                }
                Err(_) => {
                    // A larger capture buffer delivers audio in larger,
                    // less frequent batches. Do not fill normal intervals
                    // between those batches with extra silence.
                    let grace = std::time::Duration::from_millis(
                        u64::from(capture_buffer_ms.load(Ordering::Acquire)) * 2 + 40,
                    );
                    if last_pcm.elapsed() < grace {
                        return None;
                    }
                    Ok(silence.clone())
                }
            };
            stream_progress.fetch_add(1, Ordering::Relaxed);
            Some(pcm)
        })
        .take_while({
            let output = state.output().subscribe();
            let service_enabled = state.service_enabled.clone();
            move |_| {
                service_enabled.load(Ordering::Acquire)
                    && output.borrow().nonce.as_deref() == Some(nonce.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The channels field of a RIFF header sits at byte offset 22.
    fn header_channels(header: &Bytes) -> u16 {
        u16::from_le_bytes([header[22], header[23]])
    }

    #[test]
    fn the_wav_header_and_silence_follow_the_channel_plan() {
        let auto = radio_plan(ChannelMode::Auto);
        assert_eq!(
            u16::try_from(auto.channels()).unwrap(),
            PIPELINE_CHANNELS as u16
        );
        assert_eq!(header_channels(&wav_header(44_100, &auto)), 2);
        assert_eq!(silence_chunk(44_100, &auto).len() % 4, 0);
        let mono = radio_plan(ChannelMode::Mono);
        assert_eq!(mono.channels(), 1);
        // A genuine mono WAV: one channel in the header, mono silence —
        // never a duplicated mono pair.
        assert_eq!(header_channels(&wav_header(44_100, &mono)), 1);
        assert_eq!(silence_chunk(44_100, &mono).len() % 2, 0);
    }
}
