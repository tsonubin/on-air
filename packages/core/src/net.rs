//! LAN networking helpers shared by the senders, discovery loops and mDNS.

use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

/// Determines this machine's LAN-facing IP by asking the OS routing table
/// which local address it would use to reach an external address. No packet
/// is sent: UDP `connect` only fixes the default peer so the kernel picks a
/// source address.
pub fn local_lan_ip() -> std::io::Result<IpAddr> {
    local_lan_ip_toward(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)))
}

/// Source address the kernel would use to reach `peer` (e.g. a Sonos).
/// Prefer this over [`local_lan_ip`] when a VPN would otherwise win the
/// default route (198.18/15, 100.64/10) and the speaker could not fetch the
/// radio stream.
pub fn local_lan_ip_toward(peer: IpAddr) -> std::io::Result<IpAddr> {
    let bind: SocketAddr = if peer.is_ipv4() {
        "0.0.0.0:0".parse().expect("valid bind address")
    } else {
        "[::]:0".parse().expect("valid bind address")
    };
    let socket = UdpSocket::bind(bind)?;
    socket.connect(SocketAddr::new(peer, 80))?;
    Ok(socket.local_addr()?.ip())
}

/// All non-loopback IPv4 addresses of this host, best effort. Empty when the
/// interfaces cannot be enumerated. Used so the desktop can show the address
/// a phone must type when discovery is unavailable.
pub fn lan_addresses() -> Vec<String> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut addresses: Vec<String> = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(ip.to_string()),
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

/// HTTP client for LAN peers (SOAP, OwnTone, device descriptions). It must
/// not follow `HTTP_PROXY`: a system proxy would intercept 192.168.x UPnP
/// and ZoneGroupTopology traffic.
pub fn lan_http_client(timeout: Duration) -> Client {
    Client::builder()
        .timeout(timeout)
        .no_proxy()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .build()
        .expect("reqwest LAN client")
}
