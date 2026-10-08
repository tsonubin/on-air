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
    pub is_leader: bool,
    pub in_group: bool,
    pub password: bool,
    pub ip: IpAddr,
}

pub fn group_key(gid: &str) -> String {
    gid.split('+').next().unwrap_or(gid).to_string()
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
        if device.in_group && !device.gid.is_empty() {
            by_group
                .entry(group_key(&device.gid))
                .or_default()
                .push(device);
        } else {
            solos.push(device);
        }
    }
    let mut out = Vec::new();
    for members in by_group.into_values() {
        if members.len() >= 2 {
            let leader = members
                .iter()
                .find(|m| m.is_leader)
                .cloned()
                .unwrap_or_else(|| members[0].clone());
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
                member_count: members.len().min(255) as u8,
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

fn txt<'a>(info: &'a mdns_sd::ServiceInfo, key: &str) -> &'a str {
    info.get_property_val_str(key).unwrap_or("")
}

fn device_from_mdns_info(info: &mdns_sd::ServiceInfo) -> Option<AirPlayDevice> {
    let ip = info
        .get_addresses()
        .iter()
        .copied()
        .find(|ip| ip.is_ipv4())
        .or_else(|| info.get_addresses().iter().copied().next())?;
    let instance = info.get_fullname();
    device_from_txt(
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
    )
}

pub async fn search_mdns(duration: Duration) -> Vec<CatalogDevice> {
    let raw = crate::mdns::browse(
        AIRPLAY_MDNS_TYPE,
        duration,
        MAX_DISCOVERED_DEVICES,
        |info| {
            let device = device_from_mdns_info(info)?;
            Some((device.id.clone(), device))
        },
    )
    .await;
    collapse_pairs(raw)
}

const EMPTY_SCANS_BEFORE_EVICTION: u8 = 3;
const SCAN_INTERVAL: Duration = Duration::from_secs(15);

/// Merge one AirPlay scan into the catalog. Returns `true` when the catalog
/// changed so the caller can diff it for join/left events. A receiver
/// survives a few empty scans because mDNS answers are lossy.
fn apply_scan(
    current: &mut Vec<CatalogDevice>,
    found: Vec<CatalogDevice>,
    consecutive_empty_scans: &mut u8,
) -> bool {
    if found.is_empty() {
        *consecutive_empty_scans = consecutive_empty_scans.saturating_add(1);
        if *consecutive_empty_scans >= EMPTY_SCANS_BEFORE_EVICTION && !current.is_empty() {
            current.clear();
            return true;
        }
        return false;
    }
    *consecutive_empty_scans = 0;
    if *current != found {
        *current = found;
        return true;
    }
    false
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
        let mut consecutive_empty_scans = 0;
        loop {
            let mut found = search_mdns(Duration::from_secs(2)).await;
            let url = owntone_base.lock().clone();
            if let Ok(owntone) =
                crate::sender::airplay::fetch_owntone_outputs_with_client(&owntone_http, &url).await
            {
                merge_owntone(&mut found, owntone);
            }
            let diff = {
                let mut current = catalog.lock();
                let before = catalog_ids(&current);
                apply_scan(&mut current, found, &mut consecutive_empty_scans)
                    .then(|| (before, catalog_ids(&current)))
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

    fn receiver(id: &str) -> CatalogDevice {
        CatalogDevice {
            id: id.into(),
            name: format!("Receiver {id}"),
            needs_pair: false,
            paired: true,
            kind: "solo",
            member_count: 1,
            address: "192.168.1.2".into(),
        }
    }

    #[test]
    fn airplay_catalog_tolerates_transient_misses_then_evicts_stale_receivers() {
        let mut current = vec![receiver("old")];
        let mut misses = 0;

        assert!(!apply_scan(&mut current, Vec::new(), &mut misses));
        assert!(!apply_scan(&mut current, Vec::new(), &mut misses));
        assert_eq!(current, vec![receiver("old")]);

        assert!(apply_scan(&mut current, vec![receiver("new")], &mut misses));
        assert_eq!(current, vec![receiver("new")]);
        assert_eq!(misses, 0);

        let mut changed = false;
        for _ in 0..EMPTY_SCANS_BEFORE_EVICTION {
            changed |= apply_scan(&mut current, Vec::new(), &mut misses);
        }
        assert!(current.is_empty());
        assert!(changed);
    }
}
