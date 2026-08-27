/// DNS-SD type advertised so the mobile app can find a desktop core on the LAN.
pub const SERVICE_TYPE: &str = "_on-air._tcp.local.";
pub const INSTANCE_NAME: &str = "on-air";

pub fn txt_records(version: &str, port: u16) -> Vec<(String, String)> {
    vec![
        ("version".into(), version.into()),
        ("port".into(), port.to_string()),
    ]
}

/// Best-effort LAN advertisement. Failure is non-fatal (manual-IP still works).
pub fn spawn_advertisement(port: u16, version: &str) -> Option<mdns_sd::ServiceDaemon> {
    let mdns = mdns_sd::ServiceDaemon::new().ok()?;
    let host = std::env::var("HOST").unwrap_or_else(|_| "on-air-host".into());
    let host_fqdn = if host.ends_with(".local.") {
        host
    } else {
        format!("{host}.local.")
    };
    let ip = crate::sender::sonos::net::local_lan_ip().ok()?;
    let records = txt_records(version, port);
    let properties: Vec<(&str, &str)> = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let service = mdns_sd::ServiceInfo::new(
        SERVICE_TYPE,
        INSTANCE_NAME,
        &host_fqdn,
        ip,
        port,
        &properties[..],
    )
    .ok()?;
    mdns.register(service).ok()?;
    Some(mdns)
}
