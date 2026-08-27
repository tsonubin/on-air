use crate::state::CoreState;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

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

pub async fn stream_audio(State(state): State<CoreState>) -> Response {
    let rx = state.audio_tx.subscribe();
    let sample_rate = *state.target_sample_rate_hz.lock().unwrap();
    let header = wav_header(sample_rate);
    // Lagged items are skipped so a slow Sonos HTTP client does not tear down
    // the live PCM body (same failure mode previously fixed on /api/ws).
    let pcm = BroadcastStream::new(rx).filter_map(|item| {
        item.ok().map(Ok::<bytes::Bytes, std::convert::Infallible>)
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
