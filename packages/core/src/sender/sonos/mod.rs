pub mod discovery;
pub mod soap;

use crate::sender::{AudioSender, SenderError};
use crate::session::ActiveOutput;
use crate::sync::SyncController;
use discovery::SonosDevice;
use soap::SonosControlClient;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SET_URI_SETTLE_DELAY: Duration = Duration::from_millis(250);
const STREAM_RECOVERY_INITIAL_DELAY: Duration = Duration::from_secs(2);
const STREAM_RECOVERY_MAX_DELAY: Duration = Duration::from_secs(30);
const STREAM_STALL_CHECKS_BEFORE_RECOVERY: u8 = 2;

pub struct SonosSender {
    device: SonosDevice,
    client: SonosControlClient,
    stream_url: String,
    stream_clients: Option<Arc<AtomicUsize>>,
    stream_progress: Option<Arc<AtomicU64>>,
    sync: Option<(Arc<SyncController>, ActiveOutput)>,
    recovery_task: Option<tokio::task::JoinHandle<()>>,
}

impl SonosSender {
    pub fn new(device: SonosDevice, http: reqwest::Client, stream_url: String) -> Self {
        SonosSender {
            device,
            client: SonosControlClient::new(http),
            stream_url,
            stream_clients: None,
            stream_progress: None,
            sync: None,
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
        self.stream_clients = Some(stream_clients);
        self.stream_progress = Some(stream_progress);
        self
    }

    /// Give the stall recovery this output's sync context (roadmap B9). A
    /// restart of a sync-group member must re-enter its group — hold to the
    /// group's presentation epoch and report `syncing` until the drift servo
    /// re-locks — instead of re-issuing `Play` at whatever wall-clock
    /// instant the stall happened to be noticed at. A sender without this
    /// context free-runs and keeps the historical fire-now recovery.
    pub fn with_sync(mut self, controller: Arc<SyncController>, output: ActiveOutput) -> Self {
        self.sync = Some((controller, output));
        self
    }

    async fn begin_playback(&self) -> Result<(), SenderError> {
        begin_playback(&self.client, &self.device, &self.stream_url).await
    }

    fn spawn_recovery_task(&mut self) {
        let Some(stream_clients) = self.stream_clients.clone() else {
            return;
        };
        let Some(stream_progress) = self.stream_progress.clone() else {
            return;
        };
        if let Some(task) = self.recovery_task.take() {
            task.abort();
        }
        let client = self.client.clone();
        let device = self.device.clone();
        let stream_url = self.stream_url.clone();
        let sync = self.sync.clone();
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
                // A sync-group member re-enters its group instead of
                // restarting at this task's wall clock (roadmap B9): the
                // re-entry plan drops the stale drift report (the sync
                // status reports `syncing` from here) and holds the restart
                // to the group's presentation epoch — the same hold
                // `AudioSender::start_at` performs. Free-running outputs
                // have no epoch and keep the fire-now restart.
                let plan = sync.as_ref().and_then(|(controller, output)| {
                    controller.plan_resync(output, Instant::now())
                });
                if let Some(plan) = plan {
                    plan.wait().await;
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
        self.begin_playback().await?;
        self.spawn_recovery_task();
        Ok(())
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
