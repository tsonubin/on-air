pub mod discovery;
pub mod net;
pub mod soap;

use crate::sender::{AudioSender, SenderError};
use discovery::SonosDevice;
use soap::SonosControlClient;

pub struct SonosSender {
    device: SonosDevice,
    client: SonosControlClient,
    stream_url: String,
}

impl SonosSender {
    pub fn new(device: SonosDevice, http: reqwest::Client, stream_url: String) -> Self {
        SonosSender {
            device,
            client: SonosControlClient::new(http),
            stream_url,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for SonosSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.client
            .set_av_transport_uri(&self.device.av_transport_control_url(), &self.stream_url)
            .await
            .map_err(|e| SenderError(e.to_string()))?;
        self.client
            .play(&self.device.av_transport_control_url())
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.client
            .stop(&self.device.av_transport_control_url())
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.client
            .set_volume(&self.device.rendering_control_url(), volume)
            .await
            .map_err(|e| SenderError(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.device.friendly_name
    }
}
