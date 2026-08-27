use on_air_core::mdns::{txt_records, INSTANCE_NAME, SERVICE_TYPE};

#[test]
fn advertisement_identity_matches_spec() {
    assert_eq!(SERVICE_TYPE, "_on-air._tcp.local.");
    assert_eq!(INSTANCE_NAME, "on-air");
    let txt = txt_records("0.1.0", 47990);
    assert_eq!(txt.iter().find(|(k, _)| k == "version").unwrap().1, "0.1.0");
    assert_eq!(txt.iter().find(|(k, _)| k == "port").unwrap().1, "47990");
}

#[test]
fn loopback_backend_is_named_for_this_os() {
    let backend = on_air_core::pipeline::capture::loopback_backend();
    assert!(
        backend.contains("pipewire")
            || backend.contains("coreaudio")
            || backend.contains("wasapi")
            || backend == "cpal-default"
    );
}
