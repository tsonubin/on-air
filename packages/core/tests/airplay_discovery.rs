use on_air_core::sender::airplay_mdns::{collapse_pairs, device_from_txt, skip_airplay_model};
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn skips_sonos_airplay_ads_and_macs() {
    assert!(skip_airplay_model("One SL", "Sonos"));
    assert!(skip_airplay_model("Mac17,6", ""));
    assert!(!skip_airplay_model("AudioAccessory5,1", ""));
    assert!(!skip_airplay_model("AppleTV11,1", ""));
}

#[test]
fn stereo_homepods_collapse_to_the_group_leader() {
    let leader = device_from_txt(
        "卧室._airplay._tcp.local.",
        "EE:C7:74:A7:D8:56",
        "AudioAccessory5,1",
        "",
        "6D16C315-892F-5C17-BAF3-86FAC8096ED5+0+1AC1C9B3-97AC-41EF-97E7-8F9A4B12EEDE",
        "1",
        "1",
        "卧室",
        "",
        IpAddr::V4(Ipv4Addr::new(192, 168, 5, 14)),
    )
    .unwrap();
    let satellite = device_from_txt(
        "卧室._airplay._tcp.local.",
        "EE:47:0E:0C:4D:17",
        "AudioAccessory5,1",
        "",
        "6D16C315-892F-5C17-BAF3-86FAC8096ED5+0+1AC1C9B3-97AC-41EF-97E7-8F9A4B12EEDE",
        "0",
        "1",
        "卧室",
        "",
        IpAddr::V4(Ipv4Addr::new(192, 168, 5, 26)),
    )
    .unwrap();
    let listed = collapse_pairs(vec![satellite, leader]);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "EE:C7:74:A7:D8:56");
    assert_eq!(listed[0].name, "卧室");
    assert_eq!(listed[0].kind, "pair");
    assert_eq!(listed[0].member_count, 2);
    assert!(!listed[0].needs_pair);
    assert_eq!(listed[0].address, "192.168.5.14");
}

#[test]
fn apple_tv_stays_a_solo_destination() {
    let tv = device_from_txt(
        "______ (2)._airplay._tcp.local.",
        "12:A5:AE:08:9E:85",
        "AppleTV11,1",
        "",
        "195CF346-EFDB-4E1F-B69C-B2CED0F5E36C",
        "1",
        "1",
        "",
        "",
        IpAddr::V4(Ipv4Addr::new(192, 168, 5, 11)),
    )
    .unwrap();
    assert_eq!(tv.name, "Apple TV");
    let listed = collapse_pairs(vec![tv]);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Apple TV");
    assert_eq!(listed[0].kind, "solo");
    assert!(!listed[0].needs_pair);
}
