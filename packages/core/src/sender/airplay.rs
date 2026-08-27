use crate::sender::{AudioSender, SenderError};
use reqwest::Client;

/// OwnTone/forked-daapd sidecar client used on Linux and Windows.
pub struct AirPlaySender {
    name: String,
    output_id: String,
    base_url: String,
    http: Client,
}

impl AirPlaySender {
    pub fn new(name: impl Into<String>, output_id: impl Into<String>, base_url: impl Into<String>) -> Self {
        AirPlaySender {
            name: name.into(),
            output_id: output_id.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: crate::sender::sonos::soap::http_client(),
        }
    }

    async fn put_json(&self, path: &str, body: serde_json::Value) -> Result<(), SenderError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .http
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SenderError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(SenderError(format!(
                "OwnTone {} failed: {}",
                path,
                response.status()
            )))
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for AirPlaySender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.put_json(
            &format!("/api/outputs/{}", self.output_id),
            serde_json::json!({ "selected": true }),
        )
        .await?;
        self.put_json("/api/player/play", serde_json::json!({})).await
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.put_json("/api/player/stop", serde_json::json!({})).await
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.put_json(
            "/api/player/volume",
            serde_json::json!({ "volume": volume.min(100) }),
        )
        .await
    }

    fn name(&self) -> &str {
        &self.name
    }
}

pub fn platform_mode() -> &'static str {
    if cfg!(target_os = "macos") {
        "avroute-picker"
    } else {
        "owntone"
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OwnToneOutputs {
    #[serde(default)]
    outputs: Vec<OwnToneOutput>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OwnToneOutput {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    name: String,
}

pub async fn fetch_owntone_outputs(base: &str) -> Result<Vec<crate::state::CatalogDevice>, SenderError> {
    let url = format!("{}/api/outputs", base.trim_end_matches('/'));
    let body: OwnToneOutputs = crate::sender::sonos::soap::http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| SenderError(e.to_string()))?
        .json()
        .await
        .map_err(|e| SenderError(e.to_string()))?;
    Ok(body
        .outputs
        .into_iter()
        .map(|o| crate::state::CatalogDevice {
            id: match o.id {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            },
            name: o.name,
        })
        .filter(|d| !d.name.is_empty())
        .collect())
}
