use std::net::{IpAddr, SocketAddr, UdpSocket};

/// Determines this machine's LAN-facing IP by asking the OS routing table
/// which local address it would use to reach an external address — no
/// packet is actually sent (UDP `connect` just fixes the default peer and
/// lets the kernel pick a source address).
pub fn local_lan_ip() -> std::io::Result<IpAddr> {
    local_lan_ip_toward("8.8.8.8".parse().unwrap())
}

/// Source address the kernel would use to reach `peer` (e.g. a Sonos).
/// Prefer this over [`local_lan_ip`] when a VPN would otherwise win the
/// default route (198.18/15, 100.64/10) and the speaker could not fetch
/// `/stream/audio.wav`.
pub fn local_lan_ip_toward(peer: IpAddr) -> std::io::Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(SocketAddr::new(peer, 80))?;
    Ok(socket.local_addr()?.ip())
}
