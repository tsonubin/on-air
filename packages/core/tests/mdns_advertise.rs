use on_air_core::mdns::{hostname, instance_name, txt_records, INSTANCE_NAME, SERVICE_TYPE};

#[test]
fn advertisement_identity_carries_the_machine_hostname() {
    assert!(SERVICE_TYPE.starts_with("_on-air._tcp."));
    let txt = txt_records("0.1.0", 47991, "studio-mac");
    assert_eq!(txt.iter().find(|(k, _)| k == "version").unwrap().1, "0.1.0");
    assert_eq!(txt.iter().find(|(k, _)| k == "port").unwrap().1, "47991");
    assert_eq!(
        txt.iter().find(|(k, _)| k == "name").unwrap().1,
        "studio-mac"
    );
    assert_eq!(
        instance_name("studio-mac"),
        format!("{INSTANCE_NAME} @ studio-mac")
    );
}

#[test]
fn hostname_is_never_empty_and_has_no_mdns_suffix() {
    let host = hostname();
    assert!(!host.is_empty());
    assert!(!host.ends_with('.'));
    assert!(!host.ends_with(".local"));
    assert_ne!(host, "on-air-host.local.");
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

#[test]
fn loopback_names_match_os_capture_devices() {
    use on_air_core::pipeline::capture::is_loopback_device_name;
    assert!(is_loopback_device_name("alsa_output.pci.monitor"));
    assert!(is_loopback_device_name("BlackHole 2ch"));
    assert!(is_loopback_device_name("Stereo Mix"));
    assert!(!is_loopback_device_name("MacBook Pro Microphone"));
}
