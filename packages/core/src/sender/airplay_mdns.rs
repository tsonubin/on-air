//! LAN AirPlay receivers (`_airplay._tcp`), including HomePod stereo pairs.
//! OwnTone remains the Linux send path; this catalog does not depend on it.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

pub const AIRPLAY_MDNS_TYPE: &str = "_airplay._tcp.local.";
const MAX_DISCOVERED_DEVICES: usize = 64;

/// One selectable AirPlay destination as the catalog and the API see it:
/// an mDNS receiver, a collapsed HomePod stereo pair, or an OwnTone output.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirPlayDevice {
    pub id: String,
    pub name: String,
    pub model: String,
    pub gid: String,
    /// Permanent stereo identifier, independent of transient playback groups.
    pub stereo_group: String,
    pub is_leader: bool,
    pub in_group: bool,
    pub password: bool,
    pub ip: IpAddr,
}

pub fn group_key(gid: &str) -> String {
    gid.split('+')
        .next()
        .unwrap_or(gid)
        .trim()
        .to_ascii_uppercase()
}

pub fn skip_airplay_model(model: &str, manufacturer: &str) -> bool {
    let model = model.to_ascii_lowercase();
    let manufacturer = manufacturer.to_ascii_lowercase();
    manufacturer.contains("sonos") || model.contains("one sl") || model.starts_with("mac")
}

pub fn airplay_password_required(pw: &str) -> bool {
    matches!(
        pw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

/// Collapse a HomePod stereo pair to the group leader; hide the satellite.
pub fn collapse_pairs(devices: Vec<AirPlayDevice>) -> Vec<CatalogDevice> {
    let mut by_group: HashMap<String, Vec<AirPlayDevice>> = HashMap::new();
    let mut solos = Vec::new();
    for device in devices {
        if !device.stereo_group.is_empty() || (device.in_group && !device.gid.is_empty()) {
            let key = if device.stereo_group.is_empty() {
                format!("session:{}", group_key(&device.gid))
            } else {
                format!("stereo:{}", group_key(&device.stereo_group))
            };
            by_group.entry(key).or_default().push(device);
        } else {
            solos.push(device);
        }
    }
    let mut out = Vec::new();
    for mut members in by_group.into_values() {
        if members.len() >= 2 || members.iter().any(|d| !d.stereo_group.is_empty()) {
            // HomePods can both advertise igl=1. mDNS arrival order must
            // not change the selectable ID/address on every refresh.
            members.sort_by(|a, b| b.is_leader.cmp(&a.is_leader).then_with(|| a.id.cmp(&b.id)));
            let leader = members[0].clone();
            let name = if leader.name.is_empty() {
                members
                    .iter()
                    .find(|m| !m.name.is_empty())
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| leader.id.clone())
            } else {
                leader.name.clone()
            };
            out.push(CatalogDevice {
                id: leader.id,
                name,
                needs_pair: leader.password,
                paired: !leader.password,
                kind: "pair",
                member_count: members.len().clamp(2, 255) as u8,
                address: leader.ip.to_string(),
            });
        } else {
            solos.extend(members);
        }
    }
    for device in solos {
        out.push(to_catalog(device, 1, "solo"));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    out
}

fn to_catalog(device: AirPlayDevice, member_count: u8, kind: &'static str) -> CatalogDevice {
    CatalogDevice {
        id: device.id,
        name: device.name,
        needs_pair: device.password,
        paired: !device.password,
        kind,
        member_count,
        address: device.ip.to_string(),
    }
}

/// OwnTone IDs win when names match, so Play still goes through the sidecar.
pub fn merge_owntone(mdns: &mut Vec<CatalogDevice>, owntone: Vec<CatalogDevice>) {
    for ot in owntone {
        let normalized_name = normalized_device_name(&ot.name);
        if let Some(existing) = mdns.iter_mut().find(|device| {
            device.id == ot.id || normalized_device_name(&device.name) == normalized_name
        }) {
            existing.id = ot.id;
            existing.needs_pair = ot.needs_pair;
            existing.paired = ot.paired;
        } else if mdns.len() < MAX_DISCOVERED_DEVICES {
            mdns.push(ot);
        }
    }
    mdns.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
}

fn normalized_device_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

// Kept as a flat test seam mirroring the receiver's TXT fields one-for-one.
#[allow(clippy::too_many_arguments)]
pub fn device_from_txt(
    instance: &str,
    deviceid: &str,
    model: &str,
    manufacturer: &str,
    gid: &str,
    igl: &str,
    gcgl: &str,
    gpn: &str,
    pw: &str,
    ip: IpAddr,
) -> Option<AirPlayDevice> {
    if skip_airplay_model(model, manufacturer) {
        return None;
    }
    let id = if deviceid.is_empty() {
        instance.to_string()
    } else {
        deviceid.to_string()
    };
    if id.is_empty() {
        return None;
    }
    let group_name = gpn.trim();
    let instance_name = instance_display_name(instance);
    let name = if !group_name.is_empty() {
        group_name.to_string()
    } else if !instance_name.is_empty() && instance_name != "______" {
        instance_name
    } else {
        model_label(model)
    };
    let in_group = gcgl == "1" || (!gid.is_empty() && (igl == "1" || igl == "0"));
    Some(AirPlayDevice {
        id,
        name,
        model: model.to_string(),
        gid: gid.to_string(),
        stereo_group: String::new(),
        is_leader: igl == "1",
        in_group,
        password: airplay_password_required(pw),
        ip,
    })
}

fn instance_display_name(instance: &str) -> String {
    let name = instance.split('.').next().unwrap_or(instance).trim();
    let base = name.split(" (").next().unwrap_or(name).trim();
    if base.is_empty() || base.chars().all(|c| c == '_') {
        String::new()
    } else {
        name.to_string()
    }
}

fn model_label(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    if m.starts_with("audioaccessory") {
        "HomePod".into()
    } else if m.starts_with("appletv") {
        "Apple TV".into()
    } else if model.is_empty() {
        "AirPlay".into()
    } else {
        model.to_string()
    }
}

fn txt<'a>(info: &'a mdns_sd::ResolvedService, key: &str) -> &'a str {
    info.get_property_val_str(key).unwrap_or("")
}

fn device_from_mdns_info(info: &mdns_sd::ResolvedService) -> Option<AirPlayDevice> {
    let ip = info
        .get_addresses()
        .iter()
        .find(|ip| ip.is_ipv4())
        .or_else(|| info.get_addresses().iter().next())?
        .to_ip_addr();
    let instance = info.get_fullname();
    let mut device = device_from_txt(
        instance,
        txt(info, "deviceid"),
        txt(info, "model"),
        txt(info, "manufacturer"),
        txt(info, "gid"),
        txt(info, "igl"),
        txt(info, "gcgl"),
        txt(info, "gpn"),
        txt(info, "pw"),
        ip,
    )?;
    device.stereo_group = txt(info, "tsid").to_string();
    Some(device)
}

async fn search_receivers(duration: Duration) -> Vec<AirPlayDevice> {
    crate::mdns::browse(
        AIRPLAY_MDNS_TYPE,
        duration,
        MAX_DISCOVERED_DEVICES,
        |info| {
            let device = device_from_mdns_info(info)?;
            Some((device.id.clone(), device))
        },
    )
    .await
}

pub async fn search_mdns(duration: Duration) -> Vec<CatalogDevice> {
    collapse_pairs(search_receivers(duration).await)
}

const MISSED_SCANS_BEFORE_EVICTION: u8 = 3;
const SCAN_INTERVAL: Duration = Duration::from_secs(15);

/// Keep raw receivers until several scans miss each one, then derive the
/// selectable catalog. A partial scan must not break a known pair apart.
#[derive(Default)]
struct ReceiverRegistry {
    receivers: HashMap<String, (AirPlayDevice, u8)>,
}

impl ReceiverRegistry {
    fn apply_scan(&mut self, found: Vec<AirPlayDevice>) -> Vec<CatalogDevice> {
        for (_, misses) in self.receivers.values_mut() {
            *misses += 1;
        }
        for device in found {
            if self.receivers.len() < MAX_DISCOVERED_DEVICES
                || self.receivers.contains_key(&device.id)
            {
                self.receivers.insert(device.id.clone(), (device, 0));
            }
        }
        self.receivers
            .retain(|_, (_, misses)| *misses < MISSED_SCANS_BEFORE_EVICTION);
        collapse_pairs(self.receivers.values().map(|(d, _)| d.clone()).collect())
    }
}

fn catalog_ids(devices: &[CatalogDevice]) -> Vec<(String, String)> {
    devices
        .iter()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect()
}

/// The AirPlay finder: every 15 s, browse `_airplay._tcp`, merge OwnTone's
/// outputs from `owntone_base`, and replace `catalog`. Changes are broadcast
/// as `DeviceJoined`/`DeviceLeft`.
pub fn spawn(
    catalog: std::sync::Arc<parking_lot::Mutex<Vec<CatalogDevice>>>,
    owntone_base: std::sync::Arc<parking_lot::Mutex<String>>,
    ws_tx: tokio::sync::broadcast::Sender<crate::events::WsEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let owntone_http = crate::net::lan_http_client(Duration::from_millis(500));
        let mut receivers = ReceiverRegistry::default();
        loop {
            let raw = search_receivers(Duration::from_secs(2)).await;
            let mut found = receivers.apply_scan(raw);
            let url = owntone_base.lock().clone();
            if let Ok(owntone) =
                crate::sender::airplay::fetch_owntone_outputs_with_client(&owntone_http, &url).await
            {
                merge_owntone(&mut found, owntone);
            }
            let diff = {
                let mut current = catalog.lock();
                let before = catalog_ids(&current);
                if *current != found {
                    *current = found;
                    Some((before, catalog_ids(&current)))
                } else {
                    None
                }
            };
            if let Some((before, after)) = diff {
                crate::events::emit_catalog_diff(&ws_tx, "airplay", &before, &after);
            }
            tokio::time::sleep(SCAN_INTERVAL).await;
        }
    })
}

#[cfg(test)]
mod spawn_tests {
    use super::*;

    fn receiver(id: &str, leader: bool) -> AirPlayDevice {
        device_from_txt(
            "Bedroom._airplay._tcp.local.",
            id,
            "AudioAccessory5,1",
            "",
            "pair",
            if leader { "1" } else { "0" },
            "1",
            "Bedroom",
            "",
            "192.168.1.2".parse().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn partial_scans_do_not_break_a_pair_and_missing_devices_still_expire() {
        let mut registry = ReceiverRegistry::default();
        let leader = receiver("leader", true);
        let satellite = receiver("satellite", false);
        let pair = registry.apply_scan(vec![leader.clone(), satellite]);
        assert_eq!(pair.len(), 1);
        assert_eq!(pair[0].kind, "pair");
        for _ in 0..2 {
            assert_eq!(registry.apply_scan(vec![leader.clone()]), pair);
        }
        let solo = registry.apply_scan(vec![leader]);
        assert_eq!(solo[0].kind, "solo");
        assert_eq!(registry.apply_scan(vec![]), solo);
        assert_eq!(registry.apply_scan(vec![]), solo);
        assert!(registry.apply_scan(vec![]).is_empty());
    }
}
