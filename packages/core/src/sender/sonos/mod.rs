pub mod discovery;
pub mod soap;

use crate::sender::{AudioSender, SenderError};
use discovery::SonosDevice;
use soap::SonosControlClient;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SET_URI_SETTLE_DELAY: Duration = Duration::from_millis(250);
const STREAM_RECOVERY_INITIAL_DELAY: Duration = Duration::from_secs(2);
const STREAM_RECOVERY_MAX_DELAY: Duration = Duration::from_secs(30);
const STREAM_STALL_CHECKS_BEFORE_RECOVERY: u8 = 2;
const START_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_FAILURE_TIMEOUT: Duration = Duration::from_secs(15);

struct RadioHealth {
    clients: Arc<AtomicUsize>,
    progress: Arc<AtomicU64>,
    last_progress: u64,
    last_audio_at: Instant,
}

impl RadioHealth {
    fn check(&mut self, now: Instant) -> Result<(), SenderError> {
        let progress = self.progress.load(Ordering::Acquire);
        if self.clients.load(Ordering::Acquire) > 0 && progress != self.last_progress {
            self.last_audio_at = now;
            self.last_progress = progress;
        }
        if now.duration_since(self.last_audio_at) >= STREAM_FAILURE_TIMEOUT {
            return Err(SenderError::transport("Sonos stopped fetching audio and recovery did not restore it. Check the LAN connection and firewall, then retry the output."));
        }
        Ok(())
    }
}

pub struct SonosSender {
    device: SonosDevice,
    client: SonosControlClient,
    stream_url: String,
    health: Option<RadioHealth>,
    recovery_task: Option<tokio::task::JoinHandle<()>>,
}

impl SonosSender {
    pub fn new(device: SonosDevice, http: reqwest::Client, stream_url: String) -> Self {
        SonosSender {
            device,
            client: SonosControlClient::new(http),
            stream_url,
            health: None,
            recovery_task: None,
        }
    }

    /// Enable bounded recovery when Sonos stops pulling the live HTTP radio
    /// while this sender remains the selected output.
    pub fn with_stream_health(
        mut self,
        stream_clients: Arc<AtomicUsize>,
        stream_progress: Arc<AtomicU64>,
    ) -> Self {
        self.health = Some(RadioHealth {
            clients: stream_clients,
            last_progress: stream_progress.load(Ordering::Acquire),
            progress: stream_progress,
            last_audio_at: Instant::now(),
        });
        self
    }

    async fn begin_playback(&self) -> Result<(), SenderError> {
        begin_playback(&self.client, &self.device, &self.stream_url).await
    }

    fn spawn_recovery_task(&mut self) {
        let Some(health) = self.health.as_ref() else {
            return;
        };
        let stream_clients = health.clients.clone();
        let stream_progress = health.progress.clone();
        if let Some(task) = self.recovery_task.take() {
            task.abort();
        }
        let client = self.client.clone();
        let device = self.device.clone();
        let stream_url = self.stream_url.clone();
        self.recovery_task = Some(tokio::spawn(async move {
            let mut delay = STREAM_RECOVERY_INITIAL_DELAY;
            let mut last_progress = stream_progress.load(Ordering::Relaxed);
            let mut stalled_checks = 0u8;
            loop {
                tokio::time::sleep(delay).await;
                let clients = stream_clients.load(Ordering::Acquire);
                let progress = stream_progress.load(Ordering::Relaxed);
                if clients > 0 && progress != last_progress {
                    last_progress = progress;
                    stalled_checks = 0;
                    delay = STREAM_RECOVERY_INITIAL_DELAY;
                    continue;
                }
                if clients > 0 {
                    stalled_checks = stalled_checks.saturating_add(1);
                    if stalled_checks < STREAM_STALL_CHECKS_BEFORE_RECOVERY {
                        continue;
                    }
                } else {
                    stalled_checks = 0;
                }
                let _ = begin_playback(&client, &device, &stream_url).await;
                // A successful SOAP response does not prove that Sonos
                // resumed its HTTP pull. Only observed body progress resets the
                // delay; otherwise retries back off to a 30-second ceiling.
                last_progress = stream_progress.load(Ordering::Relaxed);
                delay = delay.saturating_mul(2).min(STREAM_RECOVERY_MAX_DELAY);
            }
        }));
    }

    async fn stop_recovery_task(&mut self) {
        if let Some(task) = self.recovery_task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    fn abort_recovery_task(&mut self) {
        if let Some(task) = self.recovery_task.take() {
            task.abort();
        }
    }
}

async fn begin_playback(
    client: &SonosControlClient,
    device: &SonosDevice,
    stream_url: &str,
) -> Result<(), SenderError> {
    client
        .set_av_transport_uri(&device.av_transport_control_url(), stream_url)
        .await
        .map_err(|error| SenderError::transport(error.to_string()))?;
    // Sonos closes the HTTP connection after each SOAP action
    // (`Connection: close`). A tiny pause also lets SetAVTransportURI
    // settle before Play, which otherwise races on S2.
    tokio::time::sleep(SET_URI_SETTLE_DELAY).await;
    client
        .play(&device.av_transport_control_url())
        .await
        .map_err(|error| SenderError::transport(error.to_string()))
}

impl Drop for SonosSender {
    fn drop(&mut self) {
        self.abort_recovery_task();
    }
}

#[async_trait::async_trait]
impl AudioSender for SonosSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        let initial_progress = self
            .health
            .as_ref()
            .map(|h| h.progress.load(Ordering::Acquire));
        self.begin_playback().await?;
        self.spawn_recovery_task();
        if let (Some(health), Some(initial)) = (&mut self.health, initial_progress) {
            tokio::time::timeout(START_TIMEOUT, async {
                while health.progress.load(Ordering::Acquire) == initial {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await.map_err(|_| SenderError::transport("Sonos accepted playback but did not fetch the audio stream. Check the LAN connection and the on-air firewall warning."))?;
            health.last_progress = health.progress.load(Ordering::Acquire);
            health.last_audio_at = Instant::now();
        }
        Ok(())
    }

    fn check_health(&mut self) -> Result<(), SenderError> {
        let result = self
            .health
            .as_mut()
            .map_or(Ok(()), |health| health.check(Instant::now()));
        if result.is_err() {
            self.abort_recovery_task();
        }
        result
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.stop_recovery_task().await;
        let result = self
            .client
            .stop(&self.device.av_transport_control_url())
            .await
            .map_err(|e| SenderError::transport(e.to_string()));
        if result.is_err() {
            // CoreState deliberately preserves the active sender when Stop
            // fails. Keep its recovery supervision alive for that rollback.
            self.spawn_recovery_task();
        }
        result
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.client
            .set_volume(&self.device.rendering_control_url(), volume)
            .await
            .map_err(|e| SenderError::transport(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.device.friendly_name
    }

    fn transport(&self) -> &'static str {
        "sonos"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_radio_is_given_recovery_time_then_reported_as_a_failure() {
        let now = Instant::now();
        let mut health = RadioHealth {
            clients: Arc::new(AtomicUsize::new(1)),
            progress: Arc::new(AtomicU64::new(10)),
            last_progress: 10,
            last_audio_at: now,
        };
        assert!(health.check(now + Duration::from_secs(10)).is_ok());
        health.progress.store(20, Ordering::Release);
        assert!(health.check(now + Duration::from_secs(14)).is_ok());
        health.clients.store(0, Ordering::Release);
        assert!(health.check(now + Duration::from_secs(20)).is_ok());
        assert!(health.check(now + Duration::from_secs(30)).is_err());
    }
}
