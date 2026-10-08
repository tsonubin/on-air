//! DNS-SD helpers: the `_on-air._tcp` advertisement for the phone, and a
//! generic bounded browse used by Sonos and AirPlay discovery.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// DNS-SD type advertised so the mobile app can find a desktop core on the LAN.
pub const SERVICE_TYPE: &str = "_on-air._tcp.local.";
pub const INSTANCE_NAME: &str = "on-air";
const FALLBACK_HOST: &str = "on-air-host";

/// This machine's hostname, without a trailing `.local`/`.`. Falls back to
/// `$HOSTNAME` and finally a fixed name, so advertisement never fails on it.
pub fn hostname() -> String {
    let raw = gethostname::gethostname().to_string_lossy().into_owned();
    let raw = if raw.trim().is_empty() {
        std::env::var("HOSTNAME").unwrap_or_default()
    } else {
        raw
    };
    let trimmed = raw
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(".local")
        .trim_end_matches('.');
    if trimmed.is_empty() {
        FALLBACK_HOST.to_string()
    } else {
        trimmed.to_string()
    }
}

/// `on-air @ <host>`: unique per machine so two desktops on one LAN do not
/// collide in the phone's picker.
pub fn instance_name(host: &str) -> String {
    format!("{INSTANCE_NAME} @ {host}")
}

pub fn txt_records(version: &str, port: u16, host: &str) -> Vec<(String, String)> {
    vec![
        ("version".into(), version.into()),
        ("port".into(), port.to_string()),
        ("name".into(), host.to_string()),
    ]
}

/// Best-effort LAN advertisement on the port the server actually bound.
/// Failure is non-fatal (manual-IP still works).
pub fn spawn_advertisement(port: u16, version: &str) -> Option<mdns_sd::ServiceDaemon> {
    let mdns = mdns_sd::ServiceDaemon::new().ok()?;
    let host = hostname();
    let host_fqdn = format!("{host}.local.");
    let ip = crate::net::local_lan_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_default();
    let records = txt_records(version, port, &host);
    let properties: Vec<(&str, &str)> = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let service = mdns_sd::ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name(&host),
        &host_fqdn,
        ip.as_str(),
        port,
        &properties[..],
    )
    .ok()?
    .enable_addr_auto();
    mdns.register(service).ok()?;
    Some(mdns)
}

/// Browse `service_type` for `duration`, keeping at most `max` results keyed
/// by `parse`'s key (a later resolution of the same key replaces the earlier
/// one, so an address change inside the window does not leave a stale entry).
pub fn browse_blocking<K, T>(
    service_type: &str,
    duration: Duration,
    max: usize,
    parse: impl Fn(&mdns_sd::ServiceInfo) -> Option<(K, T)>,
) -> Vec<T>
where
    K: Eq + Hash,
{
    let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let Ok(rx) = mdns.browse(service_type) else {
        let _ = mdns.shutdown();
        return Vec::new();
    };
    let deadline = Instant::now() + duration;
    let mut found = HashMap::new();
    while Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some((key, value)) = parse(&info) {
                    if found.len() < max || found.contains_key(&key) {
                        found.insert(key, value);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = mdns.shutdown();
    found.into_values().collect()
}

/// [`browse_blocking`] on the blocking pool.
pub async fn browse<K, T>(
    service_type: &'static str,
    duration: Duration,
    max: usize,
    parse: impl Fn(&mdns_sd::ServiceInfo) -> Option<(K, T)> + Send + 'static,
) -> Vec<T>
where
    K: Eq + Hash + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || browse_blocking(service_type, duration, max, parse))
        .await
        .unwrap_or_default()
}
