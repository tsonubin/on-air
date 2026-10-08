use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout as tokio_timeout;

pub const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";
pub const SSDP_SEARCH_TARGET: &str = "urn:schemas-upnp-org:device:ZonePlayer:1";
pub const DEVICE_TTL: Duration = Duration::from_secs(120);
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);
const MAX_DISCOVERED_DEVICES: usize = 64;
const MAX_REGISTRY_DEVICES: usize = 128;
const MAX_DEVICE_DESCRIPTION_BYTES: usize = 256 * 1024;
const MAX_TOPOLOGY_BYTES: usize = 1024 * 1024;

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
        if let Ok(mut url) = reqwest::Url::parse(&self.location) {
            url.set_path("");
            url.set_query(None);
            url.set_fragment(None);
            return url.as_str().trim_end_matches('/').to_string();
        }
        let without_scheme = self.location.trim_start_matches("http://");
        let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
        format!("http://{host_port}")
    }

    pub fn av_transport_control_url(&self) -> String {
        format!(
            "{}/MediaRenderer/AVTransport/Control",
            self.control_base_url()
        )
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
        if !self.devices.contains_key(&device.usn) && self.devices.len() >= MAX_REGISTRY_DEVICES {
            if let Some(oldest) = self
                .devices
                .iter()
                .min_by_key(|(_, (_, last_seen))| *last_seen)
                .map(|(id, _)| id.clone())
            {
                self.devices.remove(&oldest);
            }
        }
        self.devices.insert(device.usn.clone(), (device, seen_at));
    }

    pub fn expire_stale(&mut self, now: Instant, ttl: Duration) {
        self.devices
            .retain(|_, (_, seen_at)| now.duration_since(*seen_at) < ttl);
    }

    pub fn list(&self) -> Vec<SonosDevice> {
        let mut devices: Vec<_> = self
            .devices
            .values()
            .filter(|(d, _)| d.playable)
            .map(|(d, _)| d.clone())
            .collect();
        devices.sort_by(|a, b| {
            a.friendly_name
                .cmp(&b.friendly_name)
                .then_with(|| a.usn.cmp(&b.usn))
        });
        devices
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
                    device.member_count = group.members.len().clamp(1, u8::MAX as usize) as u8;
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
        let mac: String = rest
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(12)
            .collect();
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
    let url = reqwest::Url::parse(location).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.host_str()?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()
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
    let host = match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let url = format!("http://{host}:1400/ZoneGroupTopology/Control");
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetZoneGroupState xmlns:u="urn:schemas-upnp-org:service:ZoneGroupTopology:1"></u:GetZoneGroupState>
  </s:Body>
</s:Envelope>"#;
    let response = client
        .post(&url)
        .header("Content-Type", r#"text/xml; charset="utf-8""#)
        .header(
            "SOAPACTION",
            r#""urn:schemas-upnp-org:service:ZoneGroupTopology:1#GetZoneGroupState""#,
        )
        .body(body)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = read_limited_text(response, MAX_TOPOLOGY_BYTES).await?;
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
    let location = location?;
    // Real Sonos LOCATION headers use a numeric LAN address. Reject malformed
    // or DNS-based responses before later discovery passes spend time probing
    // an unrelated endpoint.
    let location_ip = ip_from_http_location(&location)?;
    Some(SonosDevice::discovered(
        usn?,
        location,
        location_ip,
        source_ip.to_string(),
    ))
}

async fn search_to(target: SocketAddr, duration: Duration) -> std::io::Result<Vec<SonosDevice>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let msearch = build_msearch();
    socket.send_to(msearch.as_bytes(), target).await?;

    let mut found = HashMap::new();
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
                    let key = rincon_key(&device.usn);
                    if found.len() < MAX_DISCOVERED_DEVICES || found.contains_key(&key) {
                        found.insert(key, device);
                    }
                }
            }
            _ => break,
        }
    }
    let mut found: Vec<_> = found.into_values().collect();
    found.sort_by(|a, b| a.usn.cmp(&b.usn));
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
    let location_ip = ip_from_http_location(&location)?;
    let uuid = info.get_property_val_str("uuid").unwrap_or("");
    let ip = info
        .get_addresses()
        .iter()
        .copied()
        .find(|ip| ip.is_ipv4())
        .unwrap_or(location_ip);
    Some(device_from_mdns_fields(uuid, &location, ip))
}

/// Browse `_sonos._tcp` — works on LANs where SSDP multicast is filtered.
pub async fn search_mdns(duration: Duration) -> Vec<SonosDevice> {
    let mut found =
        crate::mdns::browse(SONOS_MDNS_TYPE, duration, MAX_DISCOVERED_DEVICES, |info| {
            let device = device_from_mdns_info(info)?;
            Some((rincon_key(&device.usn), device))
        })
        .await;
    found.sort_by(|a, b| a.usn.cmp(&b.usn));
    found
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
    let response = client.get(location).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = read_limited_text(response, MAX_DEVICE_DESCRIPTION_BYTES).await?;
    extract_xml_tag_text(&body, "roomName").or_else(|| extract_xml_tag_text(&body, "friendlyName"))
}

async fn read_limited_text(response: reqwest::Response, limit: usize) -> Option<String> {
    let body = crate::http::read_bounded(response, limit).await.ok()?;
    String::from_utf8(body).ok()
}

const NAME_LOOKUP_CONCURRENCY: usize = 4;
const MAX_NAME_LOOKUPS_PER_SCAN: usize = 64;

fn spawn_name_lookup(
    tasks: &mut tokio::task::JoinSet<SonosDevice>,
    http: &reqwest::Client,
    mut device: SonosDevice,
) {
    let http = http.clone();
    tasks.spawn(async move {
        if let Some(name) = fetch_friendly_name(&http, &device.location).await {
            device.friendly_name = name;
        }
        device
    });
}

/// The Sonos finder: every [`DISCOVERY_INTERVAL`], search SSDP and
/// `_sonos._tcp`, look up room names, apply the zone topology and merge the
/// result into `registry`, expiring stale ZonePlayers. Joins and departures
/// are broadcast as `DeviceJoined`/`DeviceLeft`.
pub fn spawn(
    registry: std::sync::Arc<tokio::sync::Mutex<DeviceRegistry>>,
    ws_tx: tokio::sync::broadcast::Sender<crate::events::WsEvent>,
) -> tokio::task::JoinHandle<()> {
    use crate::events::WsEvent;
    use std::collections::HashSet;

    tokio::spawn(async move {
        let http = crate::net::lan_http_client(Duration::from_secs(2));
        loop {
            let before: HashSet<String> = {
                let registry = registry.lock().await;
                registry.list().into_iter().map(|d| d.usn).collect()
            };

            let (ssdp, mdns) = tokio::join!(
                search_once(Duration::from_secs(2)),
                search_mdns(Duration::from_secs(2)),
            );
            let mut found = ssdp.unwrap_or_default();
            found.extend(mdns);
            let mut seen = HashSet::new();
            let mut pending = found
                .into_iter()
                .filter(|device| seen.insert(rincon_key(&device.usn)))
                .take(MAX_NAME_LOOKUPS_PER_SCAN);
            let mut name_tasks = tokio::task::JoinSet::new();
            for device in pending.by_ref().take(NAME_LOOKUP_CONCURRENCY) {
                spawn_name_lookup(&mut name_tasks, &http, device);
            }
            let mut named = Vec::new();
            while let Some(result) = name_tasks.join_next().await {
                if let Ok(device) = result {
                    named.push(device);
                }
                if let Some(device) = pending.next() {
                    spawn_name_lookup(&mut name_tasks, &http, device);
                }
            }
            named.sort_by(|a, b| a.usn.cmp(&b.usn));

            let topology = named
                .iter()
                .map(soap_ip)
                .find(|ip| ip.is_ipv4())
                .or_else(|| named.first().map(|d| d.ip));
            let topology = match topology {
                Some(ip) => fetch_zone_groups(&http, ip).await,
                None => None,
            };

            let now = Instant::now();
            {
                let mut registry = registry.lock().await;
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

                let after: HashSet<String> = registry.list().into_iter().map(|d| d.usn).collect();
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
