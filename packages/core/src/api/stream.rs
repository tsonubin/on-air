use crate::api::error::ApiError;
use crate::dsp::bridge::RateBridge;
use crate::dsp::channel::{ChannelMode, ChannelPlan, Destination, SinkConstruction};
use crate::dsp::resample::RateControl;
use crate::dsp::PIPELINE_CHANNELS;
use crate::state::CoreState;
use crate::sync::{DriftController, SyncFeed};
use crate::timeline::{PcmChunk, PcmDelayLine};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

const MAX_STREAM_CLIENTS: usize = 4;
const SILENCE_INTERVAL: Duration = Duration::from_millis(40);
/// How often a synchronized receiver's measured drift is published and its
/// resampler steered. Reports are cheap; one per second is plenty.
const DRIFT_REPORT_INTERVAL: Duration = Duration::from_secs(1);

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

/// Mock only (routed like `/api/mock/cd`, never on a production core): the
/// radio URL the speaker would have been handed at activation. In
/// production the nonce is a secret the speaker receives out of band; with
/// no speaker playing, an e2e client could not otherwise reach the WAV
/// surface whose channel fit it must verify (a mono mode is a genuine mono
/// stream, `auto` the historical stereo one).
#[derive(serde::Serialize)]
pub struct MockRadioUrl {
    pub url: String,
}

pub async fn mock_radio_url(
    State(state): State<CoreState>,
) -> Result<axum::Json<MockRadioUrl>, ApiError> {
    let snapshot = state.output().snapshot();
    let nonce = snapshot
        .nonce
        .clone()
        .filter(|nonce| snapshot.serves_radio(nonce));
    match nonce {
        Some(nonce) => Ok(axum::Json(MockRadioUrl {
            url: format!("/stream/{nonce}/audio.wav"),
        })),
        None => Err(ApiError::no_active_output()),
    }
}

/// One receiver's rate stage.
///
/// Free-running outputs keep the cheap path: pure passthrough at equal
/// rates, the fixed-ratio resampler otherwise. A sync-group member gets the
/// drift-corrected variable-ratio resampler and its per-output delay line —
/// the transport delay minus the group baseline, so a fast transport waits
/// for the slowest member.
enum RateStage {
    Passthrough,
    Fixed(RateBridge),
    Sync {
        bridge: RateBridge,
        delay: Option<PcmDelayLine>,
    },
}

impl RateStage {
    /// Convert one input chunk and append 0..n zero-copy output segments to
    /// `out`. The caller clears and reuses `out`: nothing allocates per
    /// chunk beyond what the rate conversion itself does.
    fn process(&mut self, pcm: Bytes, out: &mut Vec<Bytes>) {
        match self {
            RateStage::Passthrough => out.push(pcm),
            RateStage::Fixed(bridge) => {
                let resampled = bridge.process_l16_to_l16(&pcm);
                if !resampled.is_empty() {
                    out.push(Bytes::from(resampled));
                }
            }
            RateStage::Sync { bridge, delay } => {
                let resampled = bridge.process_l16_to_l16(&pcm);
                if resampled.is_empty() {
                    return;
                }
                let converted = Bytes::from(resampled);
                match delay {
                    Some(delay) => delay.push_into(converted, out),
                    None => out.push(converted),
                }
            }
        }
    }
}

/// The receiver's audio body: the timeline bus converted and (for sync
/// members) delayed, delivered as zero-copy segments — no allocation and no
/// copying per chunk beyond what the rate stage itself does.
struct PcmBody {
    inner: BroadcastStream<PcmChunk>,
    stage: RateStage,
    pending: Vec<Bytes>,
    next: usize,
}

impl Stream for PcmBody {
    type Item = Result<Bytes, std::convert::Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if this.next < this.pending.len() {
                let segment = std::mem::take(&mut this.pending[this.next]);
                this.next += 1;
                return Poll::Ready(Some(Ok(segment)));
            }
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending.clear();
                    this.next = 0;
                    this.stage.process(chunk.pcm, &mut this.pending);
                }
                // A slow reader lagged the bus: old chunks are gone, and
                // nothing here can resurrect them.
                Poll::Ready(Some(Err(_lagged))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Measures what this receiver actually consumed from the wire and steers
/// its rate stage toward the timeline. The signal is the body's own pull
/// rate: after the receiver's buffer fills, frames leave this stream at the
/// receiver's consumption rate, which is the clock drift being corrected.
struct SyncObserver {
    feed: SyncFeed,
    drift: DriftController,
    control: RateControl,
    served_frames: u64,
    next_report: Instant,
}

impl SyncObserver {
    fn observe(&mut self, frames: u64) {
        self.served_frames += frames;
        let now = Instant::now();
        self.drift.observe(self.served_frames, now);
        if now >= self.next_report {
            self.next_report = now + DRIFT_REPORT_INTERVAL;
            self.control.set_correction_ppm(self.drift.correction_ppm());
            self.feed.report(&self.drift);
        }
    }
}

pub async fn stream_audio(
    State(state): State<CoreState>,
    Path(nonce): Path<String>,
) -> Result<Response, ApiError> {
    let state = if radio_stream_is_live(&state, &nonce) {
        state
    } else if let Some(child) = state.patchbay.stream_state(&nonce).await {
        child
    } else if let Some(child) = state.group_stream_state(&nonce).await {
        // A playing sync-group member's own radio stream.
        child
    } else {
        state
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
    let quality = state.dsp_quality();

    // The radio serves Sonos and AirPlay: a WAV surface whose channel mode
    // belongs to the active output (mono becomes a genuine mono WAV; `auto`
    // is byte-identical to the historical stereo stream).
    let identity = state.output().snapshot().identity;
    let plan = radio_plan(
        identity
            .as_ref()
            .map(|output| state.channel_mode(&output.transport, &output.device_id))
            .unwrap_or_default(),
    );
    let header = wav_header(output_hz, &plan);
    let stream_channels = plan.channels();
    let frame_bytes = stream_channels * 2;

    // A sync-group member's feed: drift-corrected conversion plus its delay
    // line. A free-running output is not in any group and keeps the cheap
    // path with no delay and no steering.
    let feed = identity
        .as_ref()
        .and_then(|identity| state.sync.feed_for(identity, output_hz));
    let (stage, observer) = match feed {
        Some(feed) => {
            let (bridge, control) = RateBridge::new_steered(pipeline_hz, output_hz, &plan, quality);
            let delay = (feed.delay_frames() > 0)
                .then(|| PcmDelayLine::new(feed.delay_frames(), stream_channels));
            let observer = SyncObserver {
                feed,
                drift: DriftController::new(output_hz),
                control: control.clone(),
                served_frames: 0,
                next_report: Instant::now(),
            };
            (RateStage::Sync { bridge, delay }, Some(observer))
        }
        // Passthrough only serves the pipeline layout untouched; a channel
        // fit still needs the bridge's map.
        None if pipeline_hz == output_hz && stream_channels == PIPELINE_CHANNELS => {
            (RateStage::Passthrough, None)
        }
        None => (
            RateStage::Fixed(RateBridge::new(pipeline_hz, output_hz, &plan, quality)),
            None,
        ),
    };

    let pcm = PcmBody {
        inner: BroadcastStream::new(rx),
        stage,
        pending: Vec::new(),
        next: 0,
    };
    let silence = silence_chunk(output_hz, &plan);
    let mut silence_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + SILENCE_INTERVAL,
        SILENCE_INTERVAL,
    );
    silence_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let stream_progress = state.stream_progress.clone();
    let capture_buffer_ms = state.capture_buffer_ms.clone();
    let mut last_pcm = tokio::time::Instant::now();
    let mut observer = observer;
    // The client guard must live exactly as long as the body stream, not
    // this handler: the closure below owns it.
    let _client_guard = client_guard;
    let pcm = pcm
        .timeout_repeating(silence_interval)
        .filter_map(move |item| {
            let _ = &_client_guard;
            let pcm = match item {
                Ok(pcm) => {
                    last_pcm = tokio::time::Instant::now();
                    pcm
                }
                Err(_) => {
                    // A larger capture buffer delivers audio in larger,
                    // less frequent batches. Do not fill normal intervals
                    // between those batches with extra silence.
                    let grace = Duration::from_millis(
                        u64::from(capture_buffer_ms.load(Ordering::Acquire)) * 2 + 40,
                    );
                    if last_pcm.elapsed() < grace {
                        return None;
                    }
                    Ok(silence.clone())
                }
            };
            if let Some(observer) = observer.as_mut() {
                if let Ok(pcm) = &pcm {
                    observer.observe((pcm.len() / frame_bytes) as u64);
                }
            }
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
