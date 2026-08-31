use reqwest::Client;
use std::time::Duration;

pub const SOAP_TIMEOUT: Duration = Duration::from_secs(5);

pub fn http_client() -> Client {
    lan_client(SOAP_TIMEOUT)
}

/// LAN SOAP/HTTP must not follow `HTTP_PROXY` (Mihomo on this host intercepts
/// 192.168.x UPnP and ZoneGroupTopology).
pub fn lan_client(timeout: Duration) -> Client {
    Client::builder()
        .timeout(timeout)
        .no_proxy()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .build()
        .expect("reqwest LAN client")
}

#[derive(Debug)]
pub struct SoapError {
    pub action: &'static str,
    pub message: String,
}

impl std::fmt::Display for SoapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SOAP {} failed: {}", self.action, self.message)
    }
}

impl std::error::Error for SoapError {}

#[derive(Clone)]
pub struct SonosControlClient {
    http: Client,
}

impl SonosControlClient {
    pub fn new(http: Client) -> Self {
        SonosControlClient { http }
    }

    pub async fn set_av_transport_uri(
        &self,
        control_url: &str,
        stream_uri: &str,
    ) -> Result<(), SoapError> {
        let escaped = xml_escape(stream_uri);
        let didl = xml_escape(&format!(
            r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"><item id="-1" parentID="-1" restricted="true"><res protocolInfo="http-get:*:audio/wav:*">{stream_uri}</res><dc:title>on-air</dc:title><upnp:class>object.item.audioItem.audioBroadcast</upnp:class></item></DIDL-Lite>"#
        ));
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <CurrentURI>{escaped}</CurrentURI>
      <CurrentURIMetaData>{didl}</CurrentURIMetaData>
    </u:SetAVTransportURI>
  </s:Body>
</s:Envelope>"#
        );
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI",
            body,
            "SetAVTransportURI",
        )
        .await
    }

    pub async fn get_transport_state(&self, control_url: &str) -> Result<String, SoapError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetTransportInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
    </u:GetTransportInfo>
  </s:Body>
</s:Envelope>"#
            .to_string();
        let text = self
            .send_action_text(
                control_url,
                "urn:schemas-upnp-org:service:AVTransport:1#GetTransportInfo",
                body,
                "GetTransportInfo",
            )
            .await?;
        let state = text
            .split("<CurrentTransportState>")
            .nth(1)
            .and_then(|s| s.split("</CurrentTransportState>").next())
            .unwrap_or("")
            .to_string();
        Ok(state)
    }

    pub async fn play(&self, control_url: &str) -> Result<(), SoapError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>"#
            .to_string();
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#Play",
            body,
            "Play",
        )
        .await
    }

    pub async fn stop(&self, control_url: &str) -> Result<(), SoapError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Stop xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
    </u:Stop>
  </s:Body>
</s:Envelope>"#
            .to_string();
        self.send_action(
            control_url,
            "urn:schemas-upnp-org:service:AVTransport:1#Stop",
            body,
            "Stop",
        )
        .await
    }

    pub async fn set_volume(
        &self,
        rendering_control_url: &str,
        volume: u8,
    ) -> Result<(), SoapError> {
        let volume = volume.min(100);
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetVolume xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
      <InstanceID>0</InstanceID>
      <Channel>Master</Channel>
      <DesiredVolume>{volume}</DesiredVolume>
    </u:SetVolume>
  </s:Body>
</s:Envelope>"#
        );
        self.send_action(
            rendering_control_url,
            "urn:schemas-upnp-org:service:RenderingControl:1#SetVolume",
            body,
            "SetVolume",
        )
        .await
    }

    async fn send_action(
        &self,
        control_url: &str,
        soap_action: &str,
        body: String,
        action_name: &'static str,
    ) -> Result<(), SoapError> {
        self.send_action_text(control_url, soap_action, body, action_name)
            .await
            .map(|_| ())
    }

    async fn send_action_text(
        &self,
        control_url: &str,
        soap_action: &str,
        body: String,
        action_name: &'static str,
    ) -> Result<String, SoapError> {
        let response = self
            .http
            .post(control_url)
            .header("Content-Type", r#"text/xml; charset="utf-8""#)
            .header("SOAPACTION", format!("\"{soap_action}\""))
            .body(body)
            .send()
            .await
            .map_err(|e| SoapError {
                action: action_name,
                message: e.to_string(),
            })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.is_success() {
            Ok(text)
        } else {
            Err(SoapError {
                action: action_name,
                message: format!("HTTP {status}: {text}"),
            })
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
