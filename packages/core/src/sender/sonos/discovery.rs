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
    /// False for bonded satellites (Invisible=1) that must not be activated alone.
    pub playable: bool,
    /// Speakers in this ZonePlayer group (2 = stereo pair).
    pub member_count: u8,
}

impl SonosDevice {
    pub fn discovered(
        usn: impl Into<String>,
        location: impl Into<String>,
        ip: IpAddr,
        friendly_name: impl Into<String>,
    ) -> Self {
        let usn = canonical_usn(&usn.into());
        let location = location.into();
        let ip = ip_from_http_location(&location).unwrap_or(ip);
        SonosDevice {
            usn,
            location,
            ip,
            friendly_name: friendly_name.into(),
            playable: true,
            member_count: 1,
        }
    }
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
        self.devices
            .values()
            .filter(|(d, _)| d.playable)
            .map(|(d, _)| d.clone())
            .collect()
    }

    pub fn apply_zone_groups(&mut self, groups: &[ZoneGroup]) {
        if groups.is_empty() {
            return;
        }
        for (device, _) in self.devices.values_mut() {
            let key = rincon_key(&device.usn);
            let mut matched = false;
            for group in groups {
                if rincon_key(&group.coordinator) == key {
                    device.playable = true;
                    device.member_count = group.members.len().max(1) as u8;
                    if let Some(name) = group
                        .members
                        .iter()
                        .find(|m| rincon_key(&m.uuid) == key)
                        .map(|m| m.name.clone())
                    {
                        if !name.is_empty() {
                            device.friendly_name = name;
                        }
                    }
                    matched = true;
                    break;
                }
                if group.members.iter().any(|m| rincon_key(&m.uuid) == key) {
                    device.playable = false;
                    device.member_count = 1;
                    matched = true;
                    break;
                }
            }
            if !matched {
                device.playable = true;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneGroup {
    pub coordinator: String,
    pub members: Vec<ZoneMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneMember {
    pub uuid: String,
    pub name: String,
    pub invisible: bool,
}

/// Stable ZonePlayer id: `RINCON_<12-hex MAC>`, ignoring USN URN suffixes
/// and the trailing instance digits (`01400`) that topology XML includes
/// but some catalogs omit.
pub fn rincon_key(usn: &str) -> String {
    let raw = usn
        .trim_start_matches("uuid:")
        .split("::")
        .next()
        .unwrap_or(usn);
    let raw = raw.split(':').next().unwrap_or(raw);
    let upper = raw.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("RINCON_") {
        let mac: String = rest.chars().filter(|c| c.is_ascii_hexdigit()).take(12).collect();
        if mac.len() == 12 {
            return format!("RINCON_{mac}");
        }
    }
    raw.to_string()
}

/// One registry key per speaker so SSDP (`uuid:RINCON_…::urn:…`) and mDNS
/// (`uuid:RINCON_…01400`) collapse.
pub fn canonical_usn(usn: &str) -> String {
    let key = rincon_key(usn);
    if key.starts_with("RINCON_") {
        format!("uuid:{key}")
    } else {
        usn.split("::").next().unwrap_or(usn).to_string()
    }
}

pub fn ip_from_http_location(location: &str) -> Option<IpAddr> {
    let rest = location.strip_prefix("http://")?;
    let host = rest.split(['/', ':']).next()?;
    host.parse().ok()
}

/// SOAP always goes to the IPv4 in `location`; mDNS may list IPv6 first.
pub fn soap_ip(device: &SonosDevice) -> IpAddr {
    ip_from_http_location(&device.location).unwrap_or(device.ip)
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Parse GetZoneGroupState XML (optionally SOAP-escaped).
pub fn parse_zone_groups(xml: &str) -> Vec<ZoneGroup> {
    let xml = xml
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    let mut groups = Vec::new();
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<ZoneGroup ") {
        let after = &rest[start..];
        let Some(end) = after.find("</ZoneGroup>") else {
            break;
        };
        let block = &after[..end];
        let coordinator = attr(block, "Coordinator").unwrap_or("").to_string();
        let mut members = Vec::new();
        let mut cursor = block;
        while let Some(mstart) = cursor.find("<ZoneGroupMember ") {
            let tag_end = cursor[mstart..].find('>').map(|i| mstart + i);
            let Some(tag_end) = tag_end else { break };
            let tag = &cursor[mstart..tag_end];
            members.push(ZoneMember {
                uuid: attr(tag, "UUID").unwrap_or("").to_string(),
                name: attr(tag, "ZoneName").unwrap_or("").to_string(),
                invisible: attr(tag, "Invisible") == Some("1"),
            });
            cursor = &cursor[tag_end..];
        }
        if !coordinator.is_empty() && !members.is_empty() {
            groups.push(ZoneGroup {
                coordinator,
                members,
            });
        }
        rest = &after[end + 12..];
    }
    groups
}

pub async fn fetch_zone_groups(client: &reqwest::Client, ip: IpAddr) -> Option<Vec<ZoneGroup>> {
    let url = format!("http://{ip}:1400/ZoneGroupTopology/Control");
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetZoneGroupState xmlns:u="urn:schemas-upnp-org:service:ZoneGroupTopology:1"></u:GetZoneGroupState>
  </s:Body>
</s:Envelope>"#;
    let text = client
        .post(&url)
        .header("Content-Type", r#"text/xml; charset="utf-8""#)
        .header(
            "SOAPACTION",
            r#""urn:schemas-upnp-org:service:ZoneGroupTopology:1#GetZoneGroupState""#,
        )
        .body(body)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let groups = parse_zone_groups(&text);
    if groups.is_empty() {
        None
    } else {
        Some(groups)
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
    Some(SonosDevice::discovered(
        usn?,
        location.clone()?,
        source_ip,
        source_ip.to_string(),
    ))
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

pub const SONOS_MDNS_TYPE: &str = "_sonos._tcp.local.";

/// Build a registry entry from mDNS TXT (`uuid`, `location`) plus an address.
pub fn device_from_mdns_fields(uuid: &str, location: &str, ip: IpAddr) -> SonosDevice {
    let usn = if uuid.is_empty() {
        location.to_string()
    } else if uuid.starts_with("uuid:") {
        uuid.to_string()
    } else {
        format!("uuid:{uuid}")
    };
    SonosDevice::discovered(usn, location, ip, ip.to_string())
}

fn device_from_mdns_info(info: &mdns_sd::ServiceInfo) -> Option<SonosDevice> {
    let location = info.get_property_val_str("location")?.to_string();
    let uuid = info.get_property_val_str("uuid").unwrap_or("");
    let ip = info
        .get_addresses()
        .iter()
        .copied()
        .find(|ip| ip.is_ipv4())
        .or_else(|| ip_from_http_location(&location))
        .or_else(|| info.get_addresses().iter().copied().next())?;
    Some(device_from_mdns_fields(uuid, &location, ip))
}

fn search_mdns_blocking(duration: Duration) -> Vec<SonosDevice> {
    let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let Ok(rx) = mdns.browse(SONOS_MDNS_TYPE) else {
        return Vec::new();
    };
    let deadline = Instant::now() + duration;
    let mut found = Vec::new();
    while Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some(device) = device_from_mdns_info(&info) {
                    found.push(device);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = mdns.shutdown();
    found
}

/// Browse `_sonos._tcp` — works on LANs where SSDP multicast is filtered.
pub async fn search_mdns(duration: Duration) -> Vec<SonosDevice> {
    tokio::task::spawn_blocking(move || search_mdns_blocking(duration))
        .await
        .unwrap_or_default()
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
