use on_air_core::pipeline::capture::list_input_devices;

#[test]
fn listing_input_devices_does_not_panic() {
    let host = cpal::default_host();
    // CI/sandboxed environments may have zero input devices — that's fine,
    // this only asserts the enumeration path itself doesn't error or panic.
    let result = list_input_devices(&host);
    assert!(result.is_ok());
}
