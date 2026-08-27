use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout as tokio_timeout;

pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";
pub const SSDP_SEARCH_TARGET: &str = "urn:schemas-upnp-org:device:ZonePlayer:1";
pub const DEVICE_TTL: Duration = Duration::from_secs(120);
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct SonosDevice {
    pub usn: String,
    pub location: String,
    pub ip: IpAddr,
    pub friendly_name: String,
}

impl SonosDevice {
    fn control_base_url(&self) -> String {
        let without_scheme = self.location.trim_start_matches("http://");
        let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
        format!("http://{host_port}")
    }

    pub fn av_transport_control_url(&self) -> String {
        format!("{}/MediaRenderer/AVTransport/Control", self.control_base_url())
    }

    pub fn rendering_control_url(&self) -> String {
        format!(
            "{}/MediaRenderer/RenderingControl/Control",
            self.control_base_url()
        )
    }
}

#[derive(Default)]
pub struct DeviceRegistry {
    devices: HashMap<String, (SonosDevice, Instant)>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, device: SonosDevice, seen_at: Instant) {
        self.devices.insert(device.usn.clone(), (device, seen_at));
    }

    pub fn expire_stale(&mut self, now: Instant, ttl: Duration) {
        self.devices
            .retain(|_, (_, seen_at)| now.duration_since(*seen_at) < ttl);
    }

    pub fn list(&self) -> Vec<SonosDevice> {
        self.devices.values().map(|(d, _)| d.clone()).collect()
    }
}

fn build_msearch() -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {SSDP_MULTICAST_ADDR}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {SSDP_SEARCH_TARGET}\r\n\r\n"
    )
}

fn parse_ssdp_response(data: &[u8], source_ip: IpAddr) -> Option<SonosDevice> {
    let text = std::str::from_utf8(data).ok()?;
    let mut usn = None;
    let mut location = None;
    for line in text.split("\r\n") {
        let mut parts = line.splitn(2, ':');
        let key = parts.next()?.trim().to_ascii_uppercase();
        let Some(value) = parts.next() else { continue };
        let value = value.trim().to_string();
        match key.as_str() {
            "USN" => usn = Some(value),
            "LOCATION" => location = Some(value),
            _ => {}
        }
    }
    Some(SonosDevice {
        usn: usn?,
        location: location.clone()?,
        ip: source_ip,
        friendly_name: source_ip.to_string(),
    })
}

async fn search_to(target: SocketAddr, duration: Duration) -> std::io::Result<Vec<SonosDevice>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let msearch = build_msearch();
    socket.send_to(msearch.as_bytes(), target).await?;

    let mut found = Vec::new();
    let mut buf = [0u8; 2048];
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio_timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                if let Some(device) = parse_ssdp_response(&buf[..len], src.ip()) {
                    found.push(device);
                }
            }
            _ => break,
        }
    }
    Ok(found)
}

/// Real discovery: broadcasts M-SEARCH to the SSDP multicast group.
pub async fn search_once(duration: Duration) -> std::io::Result<Vec<SonosDevice>> {
    let target: SocketAddr = SSDP_MULTICAST_ADDR.parse().expect("valid multicast addr");
    search_to(target, duration).await
}

/// Test seam: same protocol, but unicast to an arbitrary address instead of
/// the multicast group, so tests don't depend on multicast routing in CI.
#[doc(hidden)]
pub async fn search_once_impl(
    target: SocketAddr,
    duration: Duration,
) -> std::io::Result<Vec<SonosDevice>> {
    search_to(target, duration).await
}

fn extract_xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

pub async fn fetch_friendly_name(client: &reqwest::Client, location: &str) -> Option<String> {
    let body = client.get(location).send().await.ok()?.text().await.ok()?;
    extract_xml_tag_text(&body, "roomName").or_else(|| extract_xml_tag_text(&body, "friendlyName"))
}
