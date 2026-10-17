use on_air_core::pipeline::capture::{
    list_input_devices, pick_capture_config, pump_f32le_stereo, stereo_frame,
};
use std::sync::atomic::AtomicBool;

#[test]
fn listing_input_devices_does_not_panic() {
    let host = cpal::default_host();
    // CI/sandboxed environments may have zero input devices — that's fine,
    // this only asserts the enumeration path itself doesn't error or panic.
    let result = list_input_devices(&host);
    assert!(result.is_ok());
}

#[test]
fn stereo_frame_maps_device_layouts() {
    assert_eq!(stereo_frame(&[0.3]), [0.3, 0.3]);
    assert_eq!(stereo_frame(&[0.1, -0.2]), [0.1, -0.2]);
    assert_eq!(stereo_frame(&[0.1, -0.2, 0.9, 0.9, 0.9, 0.9]), [0.1, -0.2]);
}

#[test]
fn capture_prefers_a_stereo_config_at_the_requested_rate() {
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};
    let range = |channels, format| {
        SupportedStreamConfigRange::new(
            channels,
            44_100,
            48_000,
            SupportedBufferSize::Unknown,
            format,
        )
    };
    let picked = pick_capture_config(
        vec![
            range(6, SampleFormat::F32),
            range(2, SampleFormat::I16),
            range(1, SampleFormat::F32),
        ],
        SampleFormat::F32,
        48_000,
    )
    .unwrap();
    assert_eq!((picked.channels(), picked.sample_rate()), (2, 48_000));
    assert!(
        pick_capture_config(vec![range(2, SampleFormat::F32)], SampleFormat::F32, 96_000).is_none()
    );
}

/// Hands out 3 bytes per read, so every f32 and every frame straddles reads.
struct Trickle(std::io::Cursor<Vec<u8>>);

impl std::io::Read for Trickle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(3);
        std::io::Read::read(&mut self.0, &mut buf[..n])
    }
}

#[test]
fn pulse_reads_that_end_mid_frame_keep_left_and_right() {
    use ringbuf::traits::{Consumer, Split};
    let bytes: Vec<u8> = [0.5f32, -0.5]
        .repeat(100)
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let (mut producer, mut consumer) = ringbuf::HeapRb::<f32>::new(1024).split();
    pump_f32le_stereo(
        Trickle(std::io::Cursor::new(bytes)),
        &mut producer,
        &AtomicBool::new(false),
    );
    let samples: Vec<f32> = consumer.pop_iter().collect();
    assert_eq!(samples.len(), 200);
    assert!(samples.as_chunks::<2>().0.iter().all(|f| *f == [0.5, -0.5]));
}
