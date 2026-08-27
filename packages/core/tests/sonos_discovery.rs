use on_air_core::sender::sonos::discovery::{fetch_friendly_name, search_once, DeviceRegistry, SonosDevice};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[test]
fn device_registry_expires_stale_entries() {
    let mut registry = DeviceRegistry::new();
    let device = SonosDevice {
        usn: "uuid:test-device".into(),
        location: "http://127.0.0.1:1400/xml/device_description.xml".into(),
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        friendly_name: "Test Speaker".into(),
    };
    let t0 = Instant::now();
    registry.upsert(device.clone(), t0);
    assert_eq!(registry.list().len(), 1);

    let later = t0 + Duration::from_secs(200);
    registry.expire_stale(later, Duration::from_secs(120));
    assert_eq!(registry.list().len(), 0);
}

#[test]
fn control_urls_are_derived_from_location() {
    let device = SonosDevice {
        usn: "uuid:test-device".into(),
        location: "http://192.168.1.50:1400/xml/device_description.xml".into(),
        ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
        friendly_name: "Living Room".into(),
    };
    assert_eq!(
        device.av_transport_control_url(),
        "http://192.168.1.50:1400/MediaRenderer/AVTransport/Control"
    );
    assert_eq!(
        device.rendering_control_url(),
        "http://192.168.1.50:1400/MediaRenderer/RenderingControl/Control"
    );
}

#[tokio::test]
async fn search_once_discovers_a_fake_responder() {
    // Fake Sonos device: replies to any UDP datagram with a crafted SSDP response.
    let responder_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let responder_addr = responder_socket.local_addr().unwrap();

    // Fake device_description.xml server.
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http_listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = http_listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let xml = "<root><device><roomName>Living Room</roomName></device></root>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\n\r\n{}",
                xml.len(),
                xml
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        if let Ok((_, src)) = responder_socket.recv_from(&mut buf).await {
            let reply = format!(
                "HTTP/1.1 200 OK\r\nUSN: uuid:fake-sonos-1\r\nLOCATION: http://{http_addr}/xml/device_description.xml\r\nST: urn:schemas-upnp-org:device:ZonePlayer:1\r\n\r\n"
            );
            let _ = responder_socket.send_to(reply.as_bytes(), src).await;
        }
    });

    // search_once binds its own ephemeral socket and unicasts M-SEARCH to the
    // fake responder's address directly (loopback stand-in for the multicast group).
    let devices = search_once_to(responder_addr, Duration::from_secs(2))
        .await
        .unwrap();

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].usn, "uuid:fake-sonos-1");

    let client = reqwest::Client::new();
    let name = fetch_friendly_name(&client, &devices[0].location).await;
    assert_eq!(name.as_deref(), Some("Living Room"));
}

// Test-only helper: same as `search_once` but targets an arbitrary unicast
// address instead of the SSDP multicast group, so the test doesn't depend on
// multicast routing being available in CI.
async fn search_once_to(
    target: std::net::SocketAddr,
    timeout: Duration,
) -> std::io::Result<Vec<SonosDevice>> {
    on_air_core::sender::sonos::discovery::search_once_impl(target, timeout).await
}
