use crate::state::CoreState;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub async fn stream_audio(State(state): State<CoreState>) -> Response {
    let rx = state.audio_tx.subscribe();
    let sample_rate = *state.target_sample_rate_hz.lock().unwrap();
    // Lagged items are skipped so a slow Sonos HTTP client does not tear down
    // the live PCM body (same failure mode previously fixed on /api/ws).
    let body = Body::from_stream(BroadcastStream::new(rx).filter_map(|item| {
        item.ok().map(Ok::<bytes::Bytes, std::convert::Infallible>)
    }));

    (
        [
            (
                header::CONTENT_TYPE,
                format!("audio/L16;rate={sample_rate};channels=1"),
            ),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}
