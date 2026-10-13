//! Loopback HTTP client for the core. Routes, bodies and timeouts match
//! `packages/control-client/src/http.ts`; the TUI never needs a pairing
//! token because the core trusts loopback callers.

use crate::api::{
    ActiveInputResponse, ActiveOutputResponse, ActiveOutputView, AirPlayModeResponse, ApiErrorBody,
    BluetoothDeviceInfo, BluetoothListResponse, CdAction, CdStatus, EqResponse, InputsResponse,
    OutputInfo, OutputVolumeResponse, OutputsResponse, PinResponse, SampleRateResponse,
    ServiceResponse, StatusResponse,
};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::json;
use std::time::Duration;

/// Budget for reads and simple writes.
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Budget for calls that wait on a device handshake or on audio stopping.
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(60);

pub const DEFAULT_BASE: &str = "http://127.0.0.1:47990";

#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// No response at all: the core is down or the port is closed.
    Unreachable(String),
    Timeout,
    /// A non-2xx response; `code` and `message` come from the envelope.
    Http {
        status: u16,
        code: Option<String>,
        message: String,
    },
}

#[derive(Clone)]
pub struct Client {
    base: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: &str) -> Self {
        Client {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<impl Serialize>,
        timeout: Duration,
    ) -> Result<T, ClientError> {
        let url = format!("{}{}", self.base, path);
        let mut request = self.http.request(method, &url).timeout(timeout);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                ClientError::Timeout
            } else {
                ClientError::Unreachable(self.base.clone())
            }
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| {
            if error.is_timeout() {
                ClientError::Timeout
            } else {
                ClientError::Unreachable(self.base.clone())
            }
        })?;
        if !status.is_success() {
            let envelope = serde_json::from_slice::<ApiErrorBody>(&bytes).ok();
            return Err(ClientError::Http {
                status: status.as_u16(),
                code: envelope.as_ref().map(|e| e.code.clone()),
                message: envelope
                    .map(|e| e.error)
                    .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned()),
            });
        }
        // Empty and 204 replies deserialize to `()`; everything else is JSON.
        let text = if bytes.is_empty() { "null" } else { "" };
        let value = if text.is_empty() {
            serde_json::from_slice::<T>(&bytes)
        } else {
            serde_json::from_str::<T>(text)
        };
        value.map_err(|error| ClientError::Http {
            status: status.as_u16(),
            code: None,
            message: format!("unexpected reply from {path}: {error}"),
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        self.request(Method::GET, path, None::<()>, READ_TIMEOUT)
            .await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: impl Serialize,
        timeout: Duration,
    ) -> Result<T, ClientError> {
        self.request(method, path, Some(body), timeout).await
    }

    pub async fn status(&self) -> Result<StatusResponse, ClientError> {
        self.get("/api/status").await
    }

    pub async fn pin(&self) -> Result<String, ClientError> {
        self.get::<PinResponse>("/api/pairing/pin")
            .await
            .map(|r| r.pin)
    }

    pub async fn inputs(&self) -> Result<Vec<String>, ClientError> {
        self.get::<InputsResponse>("/api/inputs")
            .await
            .map(|r| r.inputs)
    }

    pub async fn active_input(&self) -> Result<ActiveInputResponse, ClientError> {
        self.get("/api/inputs/active").await
    }

    pub async fn activate_input(&self, name: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/inputs/active",
            json!({ "name": name }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn outputs(&self) -> Result<Vec<OutputInfo>, ClientError> {
        self.get::<OutputsResponse>("/api/outputs")
            .await
            .map(|r| r.outputs)
    }

    pub async fn active_output(&self) -> Result<Option<ActiveOutputView>, ClientError> {
        self.get::<Option<ActiveOutputResponse>>("/api/outputs/active")
            .await
            .map(|r| r.and_then(|r| r.active))
    }

    pub async fn activate_output(
        &self,
        transport: &str,
        device_id: &str,
    ) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/outputs/active",
            json!({ "transport": transport, "device_id": device_id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn deactivate_output(&self) -> Result<(), ClientError> {
        self.request(
            Method::DELETE,
            "/api/outputs/active",
            None::<()>,
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn volume(&self) -> Result<u8, ClientError> {
        self.get::<OutputVolumeResponse>("/api/outputs/active/volume")
            .await
            .map(|r| r.volume)
    }

    pub async fn set_volume(&self, volume: u8) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/outputs/active/volume",
            json!({ "volume": volume }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn eq(&self) -> Result<[f32; 5], ClientError> {
        self.get::<EqResponse>("/api/eq").await.map(|r| r.gains_db)
    }

    pub async fn set_eq(&self, gains_db: [f32; 5]) -> Result<(), ClientError> {
        self.send_json(
            Method::PUT,
            "/api/eq",
            json!({ "gains_db": gains_db }),
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn sample_rate(&self) -> Result<SampleRateResponse, ClientError> {
        self.get("/api/sample-rate").await
    }

    pub async fn set_sample_rate(
        &self,
        input_hz: Option<u32>,
        output_hz: Option<u32>,
    ) -> Result<(), ClientError> {
        let mut body = serde_json::Map::new();
        if let Some(hz) = input_hz {
            body.insert("input_hz".into(), hz.into());
        }
        if let Some(hz) = output_hz {
            body.insert("output_hz".into(), hz.into());
        }
        self.send_json(Method::PUT, "/api/sample-rate", body, READ_TIMEOUT)
            .await
    }

    pub async fn airplay_mode(&self) -> Result<String, ClientError> {
        self.get::<AirPlayModeResponse>("/api/airplay/mode")
            .await
            .map(|r| r.mode)
    }

    pub async fn pair_airplay(&self, device_id: &str, pin: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/airplay/pair",
            json!({ "device_id": device_id, "pin": pin }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn bluetooth_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, ClientError> {
        self.get::<BluetoothListResponse>("/api/bluetooth/devices")
            .await
            .map(|r| r.devices)
    }

    pub async fn pair_bluetooth(&self, id: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/bluetooth/pair",
            json!({ "id": id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn connect_bluetooth(&self, id: &str) -> Result<(), ClientError> {
        self.send_json(
            Method::POST,
            "/api/bluetooth/connect",
            json!({ "id": id }),
            SLOW_TIMEOUT,
        )
        .await
    }

    pub async fn open_bluetooth_settings(&self) -> Result<(), ClientError> {
        self.request(
            Method::POST,
            "/api/bluetooth/settings",
            None::<()>,
            READ_TIMEOUT,
        )
        .await
    }

    pub async fn cd(&self) -> Result<CdStatus, ClientError> {
        self.get("/api/cd").await
    }

    pub async fn control_cd(&self, action: CdAction) -> Result<(), ClientError> {
        self.send_json::<CdStatus>(
            Method::POST,
            "/api/cd/control",
            json!({ "action": action }),
            READ_TIMEOUT,
        )
        .await
        .map(|_| ())
    }

    pub async fn set_service(&self, enabled: bool) -> Result<bool, ClientError> {
        self.send_json::<ServiceResponse>(
            Method::PUT,
            "/api/service",
            json!({ "enabled": enabled }),
            SLOW_TIMEOUT,
        )
        .await
        .map(|r| r.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_slash_is_trimmed_from_the_base() {
        assert_eq!(Client::new("http://x:1/").base(), "http://x:1");
    }
}
