//! LAN AirPlay receivers (`_airplay._tcp`), including HomePod stereo pairs.
//! OwnTone remains the Linux send path; this catalog does not depend on it.

use crate::state::CatalogDevice;
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

pub const AIRPLAY_MDNS_TYPE: &str = "_airplay._tcp.local.";

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
    manufacturer.contains("sonos")
        || model.contains("one sl")
        || model.starts_with("mac")
}

pub fn airplay_password_required(pw: &str) -> bool {
    matches!(pw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

/// Collapse a HomePod stereo pair to the group leader; hide the satellite.
pub fn collapse_pairs(devices: Vec<AirPlayDevice>) -> Vec<CatalogDevice> {
    let mut by_group: HashMap<String, Vec<AirPlayDevice>> = HashMap::new();
    let mut solos = Vec::new();
    for device in devices {
        if device.in_group && !device.gid.is_empty() {
            by_group.entry(group_key(&device.gid)).or_default().push(device);
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
    out.sort_by(|a, b| a.name.cmp(&b.name));
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
        if let Some(existing) = mdns.iter_mut().find(|d| d.name == ot.name) {
            existing.id = ot.id;
            existing.needs_pair = ot.needs_pair;
            existing.paired = ot.paired;
        } else {
            mdns.push(ot);
        }
    }
}

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

fn search_mdns_blocking(duration: Duration) -> Vec<AirPlayDevice> {
    let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let Ok(rx) = mdns.browse(AIRPLAY_MDNS_TYPE) else {
        return Vec::new();
    };
    let deadline = Instant::now() + duration;
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some(device) = device_from_mdns_info(&info) {
                    if seen.insert(device.id.clone()) {
                        found.push(device);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = mdns.shutdown();
    found
}

pub async fn search_mdns(duration: Duration) -> Vec<CatalogDevice> {
    let raw = tokio::task::spawn_blocking(move || search_mdns_blocking(duration))
        .await
        .unwrap_or_default();
    collapse_pairs(raw)
}
