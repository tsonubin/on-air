use reqwest::Client;

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
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <CurrentURI>{stream_uri}</CurrentURI>
      <CurrentURIMetaData></CurrentURIMetaData>
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

    pub async fn set_volume(&self, rendering_control_url: &str, volume: u8) -> Result<(), SoapError> {
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

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(SoapError {
                action: action_name,
                message: format!("HTTP {status}: {text}"),
            })
        }
    }
}
