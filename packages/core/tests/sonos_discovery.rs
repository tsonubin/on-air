use on_air_core::sender::sonos::discovery::{
    device_from_mdns_fields, fetch_friendly_name, parse_zone_groups, search_once, DeviceRegistry,
    SonosDevice,
};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[test]
fn stereo_pair_hides_the_bonded_satellite() {
    let xml = r#"
<ZoneGroup Coordinator="RINCON_38420B56ED0E01400" ID="g1">
  <ZoneGroupMember UUID="RINCON_38420B56ECEC01400" ZoneName="主卧" Invisible="1"/>
  <ZoneGroupMember UUID="RINCON_38420B56ED0E01400" ZoneName="主卧"/>
</ZoneGroup>"#;
    let groups = parse_zone_groups(xml);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].coordinator, "RINCON_38420B56ED0E01400");
    assert_eq!(groups[0].members.len(), 2);

    let mut registry = DeviceRegistry::new();
    let t0 = Instant::now();
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ECEC01400",
            "http://192.168.5.2:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 2)),
            "主卧",
        ),
        t0,
    );
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ED0E01400",
            "http://192.168.5.3:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 3)),
            "主卧",
        ),
        t0,
    );
    registry.apply_zone_groups(&groups);
    let listed = registry.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].usn, "uuid:RINCON_38420B56ED0E");
    assert_eq!(listed[0].member_count, 2);
}

#[test]
fn stereo_pair_matches_mdns_uuid_without_instance_suffix() {
    let xml = r#"
<ZoneGroup Coordinator="RINCON_38420B56ED0E01400" ID="g1">
  <ZoneGroupMember UUID="RINCON_38420B56ECEC01400" ZoneName="主卧" Invisible="1"/>
  <ZoneGroupMember UUID="RINCON_38420B56ED0E01400" ZoneName="主卧"/>
</ZoneGroup>"#;
    let groups = parse_zone_groups(xml);
    let mut registry = DeviceRegistry::new();
    let t0 = Instant::now();
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ECEC",
            "http://192.168.5.2:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 2)),
            "主卧",
        ),
        t0,
    );
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ED0E",
            "http://192.168.5.3:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 3)),
            "主卧",
        ),
        t0,
    );
    registry.apply_zone_groups(&groups);
    let listed = registry.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].usn, "uuid:RINCON_38420B56ED0E");
    assert_eq!(listed[0].member_count, 2);
}

#[test]
fn ssdp_and_mdns_usn_collapse_to_one_registry_key() {
    let mut registry = DeviceRegistry::new();
    let t0 = Instant::now();
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ED0E01400::urn:schemas-upnp-org:device:ZonePlayer:1",
            "http://192.168.5.3:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 3)),
            "ssdp",
        ),
        t0,
    );
    registry.upsert(
        SonosDevice::discovered(
            "uuid:RINCON_38420B56ED0E01400",
            "http://192.168.5.3:1400/xml/device_description.xml",
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 3)),
            "mdns",
        ),
        t0,
    );
    assert_eq!(registry.list().len(), 1);
    assert_eq!(registry.list()[0].usn, "uuid:RINCON_38420B56ED0E");
}

#[test]
fn parse_zone_groups_reads_soap_escaped_topology() {
    let soap = r#"<s:Envelope><s:Body><u:GetZoneGroupStateResponse><ZoneGroupState>&lt;ZoneGroupState&gt;&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator=&quot;RINCON_38420B56ED0E01400&quot; ID=&quot;g1&quot;&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_38420B56ED0E01400&quot; ZoneName=&quot;主卧&quot;/&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_38420B56ECEC01400&quot; ZoneName=&quot;主卧&quot; Invisible=&quot;1&quot;/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;&lt;/ZoneGroupState&gt;</ZoneGroupState></u:GetZoneGroupStateResponse></s:Body></s:Envelope>"#;
    let groups = parse_zone_groups(soap);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        on_air_core::sender::sonos::discovery::rincon_key(&groups[0].coordinator),
        "RINCON_38420B56ED0E"
    );
    assert_eq!(groups[0].members.len(), 2);
}

#[test]
fn mdns_fields_become_a_usn_and_location() {
    let device = device_from_mdns_fields(
        "RINCON_ABC",
        "http://192.168.5.3:1400/xml/device_description.xml",
        IpAddr::V4(Ipv4Addr::new(192, 168, 5, 3)),
    );
    assert_eq!(device.usn, "uuid:RINCON_ABC");
    assert_eq!(
        device.location,
        "http://192.168.5.3:1400/xml/device_description.xml"
    );
}

#[test]
fn device_registry_expires_stale_entries() {
    let mut registry = DeviceRegistry::new();
    let device = SonosDevice::discovered(
        "uuid:test-device",
        "http://127.0.0.1:1400/xml/device_description.xml",
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        "Test Speaker",
    );
    let t0 = Instant::now();
    registry.upsert(device.clone(), t0);
    assert_eq!(registry.list().len(), 1);

    let later = t0 + Duration::from_secs(200);
    registry.expire_stale(later, Duration::from_secs(120));
    assert_eq!(registry.list().len(), 0);
}

#[test]
fn control_urls_are_derived_from_location() {
    let device = SonosDevice::discovered(
        "uuid:test-device",
        "http://192.168.1.50:1400/xml/device_description.xml",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
        "Living Room",
    );
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
