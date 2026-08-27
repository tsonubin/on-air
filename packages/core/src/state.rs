use crate::sender::sonos::discovery::DeviceRegistry;
use crate::sender::{AudioSender, SenderError};
use bytes::Bytes;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};

#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
    pub outputs: Arc<Mutex<DeviceRegistry>>,
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;

impl CoreState {
    pub fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(64);
        CoreState {
            active_sender: Arc::new(Mutex::new(None)),
            eq_gains_db: Arc::new(StdMutex::new([0.0; 5])),
            target_sample_rate_hz: Arc::new(StdMutex::new(TARGET_SAMPLE_RATE_DEFAULT_HZ)),
            audio_tx,
            capture: Arc::new(Mutex::new(None)),
            outputs: Arc::new(Mutex::new(DeviceRegistry::new())),
        }
    }

    pub async fn activate_sender(&self, new_sender: Box<dyn AudioSender>) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            current.stop().await?;
        }
        let mut new_sender = new_sender;
        new_sender.start().await?;
        *guard = Some(new_sender);
        Ok(())
    }

    pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            current.stop().await?;
        }
        Ok(())
    }

    /// Spawns a background task that periodically SSDP-searches for Sonos
    /// devices and merges results into `self.outputs`, expiring stale entries.
    pub fn spawn_sonos_discovery(&self) -> tokio::task::JoinHandle<()> {
        use crate::sender::sonos::discovery::{search_once, DEVICE_TTL, DISCOVERY_INTERVAL};

        let outputs = self.outputs.clone();
        tokio::spawn(async move {
            let http = reqwest::Client::new();
            loop {
                if let Ok(found) = search_once(Duration::from_secs(2)).await {
                    let now = Instant::now();
                    let mut registry = outputs.lock().await;
                    for mut device in found {
                        if let Some(name) =
                            crate::sender::sonos::discovery::fetch_friendly_name(&http, &device.location)
                                .await
                        {
                            device.friendly_name = name;
                        }
                        registry.upsert(device, now);
                    }
                    registry.expire_stale(now, DEVICE_TTL);
                }
                tokio::time::sleep(DISCOVERY_INTERVAL).await;
            }
        })
    }
}

impl Default for CoreState {
    fn default() -> Self {
        Self::new()
    }
}
