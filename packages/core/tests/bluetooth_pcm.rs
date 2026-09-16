use bytes::Bytes;
use on_air_core::sender::bluetooth::{
    is_coreaudio_bluetooth_transport, looks_like_a2dp_sink, looks_like_windows_bluetooth_id,
    parse_bluetoothctl_devices, parse_bluetoothctl_info, parse_pactl_list_sinks,
    pulse_sink_address, BluetoothAdapter, BluetoothDevice, BluetoothPlaybackConfig,
    BluetoothSender, MockBluetoothAdapter, RecordingPcmSink,
};
use on_air_core::sender::AudioSender;
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::test]
async fn bluetooth_sender_pumps_pipeline_pcm_into_the_sink() {
    let adapter = Arc::new(MockBluetoothAdapter::with_devices(vec![BluetoothDevice {
        id: "bt-1".into(),
        name: "BT".into(),
        paired: true,
        connected: false,
        audio_endpoint: Some("bt-1".into()),
    }]));
    let sink = Arc::new(RecordingPcmSink::default());
    let (audio_tx, _) = broadcast::channel(8);
    let mut sender = BluetoothSender::new(
        adapter.list()[0].clone(),
        adapter.clone(),
        audio_tx.clone(),
        Arc::new(sink.clone()),
        BluetoothPlaybackConfig {
            pipeline_hz: 44_100,
            output_hz: 44_100,
            volume: 100,
        },
    );
    sender.start().await.unwrap();
    assert!(adapter.list()[0].connected);

    let pcm = Bytes::from_static(&[0x00, 0x10, 0x00, 0x20]);
    audio_tx.send(pcm.clone()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(
        sink.byte_count() >= pcm.len(),
        "expected pipeline PCM to reach the bluetooth sink, got {}",
        sink.byte_count()
    );
    sender.stop().await.unwrap();
}

#[test]
fn pactl_listing_keeps_only_bluez_sinks() {
    let text = "\
Sink #56
\tName: alsa_output.pci-0000_00_1b.0.analog-stereo
\tDescription: Built-in Audio Analog Stereo
Sink #706
\tName: bluez_output.58_EA_1F_87_56_45.1
\tDescription: Xiaomi Speaker Mini-8230
";
    let devices = parse_pactl_list_sinks(text);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, "58:EA:1F:87:56:45");
    assert_eq!(
        devices[0].audio_endpoint.as_deref(),
        Some("bluez_output.58_EA_1F_87_56_45.1")
    );
    assert_eq!(devices[0].name, "Xiaomi Speaker Mini-8230");
    assert!(devices[0].connected);
    assert!(!looks_like_a2dp_sink("HDA Intel HDMI, HDMI 0"));
    assert!(looks_like_a2dp_sink("bluez_output.aa.1"));
}

#[test]
fn pulse_sink_address_reads_bluez_mac() {
    assert_eq!(
        pulse_sink_address("bluez_output.58_EA_1F_87_56_45.a2dp_sink").as_deref(),
        Some("58:EA:1F:87:56:45")
    );
    assert_eq!(
        pulse_sink_address("alsa_output.pci-0000_00_1b.0.analog-stereo"),
        None
    );
}

#[test]
fn bluetoothctl_parser_keeps_audio_sinks() {
    let listed = parse_bluetoothctl_devices(
        "Device 58:EA:1F:87:56:45 Xiaomi Speaker Mini-8230\nDevice 00:11:22:33:44:55 Keyboard\n",
    );
    assert_eq!(listed[0].0, "58:EA:1F:87:56:45");
    assert_eq!(listed[0].1, "Xiaomi Speaker Mini-8230");
    let info = parse_bluetoothctl_info(
        "Paired: yes\nConnected: no\nUUID: Audio Sink (0000110b-0000-1000-8000-00805f9b34fb)\n",
    );
    assert!(info.paired);
    assert!(!info.connected);
    assert!(info.audio_sink);
}

#[test]
fn platform_filters_keep_bluetooth_transports() {
    assert!(is_coreaudio_bluetooth_transport(u32::from_be_bytes(
        *b"blue"
    )));
    assert!(is_coreaudio_bluetooth_transport(u32::from_be_bytes(
        *b"blea"
    )));
    assert!(!is_coreaudio_bluetooth_transport(u32::from_be_bytes(
        *b"hdmi"
    )));
    assert!(looks_like_windows_bluetooth_id(
        r"\\?\BTHENUM#{0000110b-0000-1000-8000-00805f9b34fb}"
    ));
    assert!(!looks_like_windows_bluetooth_id("{0.0.0.00000000}.{abcd}"));
}
