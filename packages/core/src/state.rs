use crate::api::ws::WsEvent;
use crate::pairing::PairingState;
use crate::sender::bluetooth::{
    BluetoothAdapter, BluetoothDevice, MockBluetoothAdapter, RecordingPcmSink, SystemBluetoothAdapter,
};
use crate::sender::sonos::discovery::{DeviceRegistry, SonosDevice};
use crate::sender::{AudioSender, SenderError};
use bytes::Bytes;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDevice {
    pub id: String,
    pub name: String,
    pub needs_pair: bool,
    pub paired: bool,
    pub kind: &'static str,
    pub member_count: u8,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ActiveOutput {
    pub transport: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone)]
pub struct CoreState {
    pub active_sender: Arc<Mutex<Option<Box<dyn AudioSender>>>>,
    pub eq_gains_db: Arc<StdMutex<[f32; 5]>>,
    pub target_sample_rate_hz: Arc<StdMutex<u32>>,
    pub output_sample_rate_hz: Arc<StdMutex<u32>>,
    pub audio_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Option<crate::pipeline::CaptureHandle>>>,
    pub outputs: Arc<Mutex<DeviceRegistry>>,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub mock: bool,
    pub mock_inputs: Arc<StdMutex<Vec<String>>>,
    pub active_input: Arc<StdMutex<Option<String>>>,
    pub airplay_outputs: Arc<StdMutex<Vec<CatalogDevice>>>,
    pub bluetooth: Arc<dyn BluetoothAdapter>,
    pub pcm_sink: Arc<RecordingPcmSink>,
    pub require_auth: bool,
    pub pairing: Arc<StdMutex<PairingState>>,
    pub active_output: Arc<StdMutex<Option<ActiveOutput>>>,
    pub mock_log: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub owntone_base: Arc<StdMutex<String>>,
}

pub const TARGET_SAMPLE_RATE_DEFAULT_HZ: u32 = 44100;

impl CoreState {
    pub fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(64);
        let (ws_tx, _) = broadcast::channel(64);
        CoreState {
            active_sender: Arc::new(Mutex::new(None)),
            eq_gains_db: Arc::new(StdMutex::new([0.0; 5])),
            target_sample_rate_hz: Arc::new(StdMutex::new(TARGET_SAMPLE_RATE_DEFAULT_HZ)),
            output_sample_rate_hz: Arc::new(StdMutex::new(TARGET_SAMPLE_RATE_DEFAULT_HZ)),
            audio_tx,
            capture: Arc::new(Mutex::new(None)),
            outputs: Arc::new(Mutex::new(DeviceRegistry::new())),
            ws_tx,
            mock: false,
            mock_inputs: Arc::new(StdMutex::new(Vec::new())),
            active_input: Arc::new(StdMutex::new(None)),
            airplay_outputs: Arc::new(StdMutex::new(Vec::new())),
            bluetooth: Arc::new(SystemBluetoothAdapter::from_host()),
            pcm_sink: Arc::new(RecordingPcmSink::default()),
            require_auth: false,
            pairing: Arc::new(StdMutex::new(PairingState::new())),
            active_output: Arc::new(StdMutex::new(None)),
            mock_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            owntone_base: Arc::new(StdMutex::new("http://127.0.0.1:3689".into())),
        }
    }

    /// E2E / CI: fake inputs and transports, fixed pairing PIN, no hardware.
    pub async fn new_mock() -> Self {
        let mut state = Self::new();
        state.mock = true;
        state.pairing = Arc::new(StdMutex::new(PairingState::mock()));
        *state.mock_inputs.lock().unwrap() = vec!["Mock Monitor".into()];
        *state.airplay_outputs.lock().unwrap() = vec![CatalogDevice {
            id: "ap-living".into(),
            name: "Living Room AirPlay".into(),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: String::new(),
        }];
        state.bluetooth = Arc::new(MockBluetoothAdapter::with_devices(vec![BluetoothDevice {
            id: "bt-speaker".into(),
            name: "Mock Bluetooth Speaker".into(),
            paired: true,
            connected: false,
        }]));
        let sonos = SonosDevice::discovered(
            "uuid:mock-sonos",
            "http://127.0.0.1:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "Mock Sonos",
        );
        state.outputs.lock().await.upsert(sonos, Instant::now());
        state
    }

    pub fn spawn_airplay_discovery(&self) -> tokio::task::JoinHandle<()> {
        let outputs = self.airplay_outputs.clone();
        let base = self.owntone_base.clone();
        tokio::spawn(async move {
            loop {
                let mut found = crate::sender::airplay_mdns::search_mdns(Duration::from_secs(2)).await;
                let url = base.lock().unwrap().clone();
                if let Ok(owntone) = crate::sender::airplay::fetch_owntone_outputs(&url).await {
                    crate::sender::airplay_mdns::merge_owntone(&mut found, owntone);
                }
                if !found.is_empty() {
                    *outputs.lock().unwrap() = found;
                }
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        })
    }

    pub async fn activate_sender(&self, new_sender: Box<dyn AudioSender>) -> Result<(), SenderError> {
        self.activate_sender_as(new_sender, None).await
    }

    pub async fn activate_sender_as(
        &self,
        new_sender: Box<dyn AudioSender>,
        identity: Option<ActiveOutput>,
    ) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            let previous = self.active_output.lock().unwrap().take();
            let transport = previous
                .as_ref()
                .map(|o| o.transport.clone())
                .unwrap_or_else(|| current.transport().to_string());
            let device_name = previous
                .as_ref()
                .map(|o| o.device_name.clone())
                .unwrap_or_else(|| current.name().to_string());
            let _ = current.stop().await;
            let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
                transport,
                device_name,
                active: false,
            });
        }
        let mut new_sender = new_sender;
        let identity = identity.unwrap_or_else(|| ActiveOutput {
            transport: new_sender.transport().to_string(),
            device_id: String::new(),
            device_name: new_sender.name().to_string(),
        });
        // GET /stream/audio.wav is 404 until this is set. Sonos Play pulls
        // the URI immediately, so the radio must be live before SOAP starts.
        *self.active_output.lock().unwrap() = Some(identity.clone());
        if let Err(e) = new_sender.start().await {
            *self.active_output.lock().unwrap() = None;
            if !self.mock {
                crate::pipeline::local_sink::restore_local_speakers();
            }
            return Err(e);
        }
        if !self.mock {
            crate::pipeline::local_sink::apply_for_transport(
                &identity.transport,
                &identity.device_id,
            );
        }
        let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
            transport: identity.transport.clone(),
            device_name: identity.device_name.clone(),
            active: true,
        });
        *guard = Some(new_sender);
        Ok(())
    }

    pub async fn deactivate_sender(&self) -> Result<(), SenderError> {
        let mut guard = self.active_sender.lock().await;
        if let Some(mut current) = guard.take() {
            let previous = self.active_output.lock().unwrap().take();
            let transport = previous
                .as_ref()
                .map(|o| o.transport.clone())
                .unwrap_or_else(|| current.transport().to_string());
            let device_name = previous
                .as_ref()
                .map(|o| o.device_name.clone())
                .unwrap_or_else(|| current.name().to_string());
            current.stop().await?;
            if !self.mock {
                crate::pipeline::local_sink::restore_local_speakers();
            }
            let _ = self.ws_tx.send(WsEvent::OutputStateChanged {
                transport,
                device_name,
                active: false,
            });
        }
        Ok(())
    }

    /// Spawns a background task that periodically SSDP-searches for Sonos
    /// devices and merges results into `self.outputs`, expiring stale entries.
    pub fn spawn_sonos_discovery(&self) -> tokio::task::JoinHandle<()> {
        use crate::sender::sonos::discovery::{
            search_mdns, search_once, DEVICE_TTL, DISCOVERY_INTERVAL,
        };

        let outputs = self.outputs.clone();
        let ws_tx = self.ws_tx.clone();
        tokio::spawn(async move {
            let http = crate::sender::sonos::soap::lan_client(Duration::from_secs(2));
            loop {
                let before: std::collections::HashSet<String> = {
                    let registry = outputs.lock().await;
                    registry.list().into_iter().map(|d| d.usn).collect()
                };

                let (ssdp, mdns) = tokio::join!(
                    search_once(Duration::from_secs(2)),
                    search_mdns(Duration::from_secs(2)),
                );
                let mut found = ssdp.unwrap_or_default();
                found.extend(mdns);
                let mut named = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for mut device in found {
                    if !seen.insert(crate::sender::sonos::discovery::rincon_key(&device.usn)) {
                        continue;
                    }
                    if let Some(name) =
                        crate::sender::sonos::discovery::fetch_friendly_name(&http, &device.location)
                            .await
                    {
                        device.friendly_name = name;
                    }
                    named.push(device);
                }

                let topology = named
                    .iter()
                    .map(crate::sender::sonos::discovery::soap_ip)
                    .find(|ip| ip.is_ipv4())
                    .or_else(|| named.first().map(|d| d.ip));
                let topology = match topology {
                    Some(ip) => {
                        crate::sender::sonos::discovery::fetch_zone_groups(&http, ip).await
                    }
                    None => None,
                };

                let now = Instant::now();
                {
                    let mut registry = outputs.lock().await;
                    for device in named {
                        if !before.contains(&device.usn) {
                            let _ = ws_tx.send(WsEvent::DeviceJoined {
                                transport: "sonos".to_string(),
                                id: device.usn.clone(),
                                name: device.friendly_name.clone(),
                            });
                        }
                        registry.upsert(device, now);
                    }
                    if let Some(groups) = topology {
                        registry.apply_zone_groups(&groups);
                    }
                    registry.expire_stale(now, DEVICE_TTL);

                    let after: std::collections::HashSet<String> =
                        registry.list().into_iter().map(|d| d.usn).collect();
                    for left in before.difference(&after) {
                        let _ = ws_tx.send(WsEvent::DeviceLeft {
                            transport: "sonos".to_string(),
                            id: left.clone(),
                        });
                    }
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
