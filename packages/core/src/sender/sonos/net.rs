use std::net::{IpAddr, UdpSocket};

/// Determines this machine's LAN-facing IP by asking the OS routing table
/// which local address it would use to reach an external address — no
/// packet is actually sent (UDP `connect` just fixes the default peer and
/// lets the kernel pick a source address).
pub fn local_lan_ip() -> std::io::Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip())
}
