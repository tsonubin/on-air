//! Bounded HTTP body reading shared by the Sonos and OwnTone catalog fetches.

use reqwest::Response;

#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    #[error("response exceeds {limit} bytes")]
    TooLarge { limit: usize },
    #[error("read response: {0}")]
    Read(#[from] reqwest::Error),
}

/// Read a response body into memory, failing once it exceeds `limit` bytes
/// so a misbehaving LAN peer cannot exhaust memory.
pub async fn read_bounded(mut response: Response, limit: usize) -> Result<Vec<u8>, BodyError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(BodyError::TooLarge { limit });
    }
    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .min(limit as u64) as usize,
    );
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(BodyError::TooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
