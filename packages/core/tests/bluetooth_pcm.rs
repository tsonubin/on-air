use bytes::Bytes;
use on_air_core::sender::bluetooth::{
    looks_like_a2dp_sink, parse_pactl_list_sinks, BluetoothAdapter, BluetoothDevice,
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
    }]));
    let sink = Arc::new(RecordingPcmSink::default());
    let (audio_tx, _) = broadcast::channel(8);
    let mut sender = BluetoothSender::new("bt-1", "BT", adapter.clone(), audio_tx.clone(), sink.clone());
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
    assert_eq!(devices[0].id, "bluez_output.58_EA_1F_87_56_45.1");
    assert_eq!(devices[0].name, "Xiaomi Speaker Mini-8230");
    assert!(!looks_like_a2dp_sink("HDA Intel HDMI, HDMI 0"));
    assert!(looks_like_a2dp_sink("bluez_output.aa.1"));
}
