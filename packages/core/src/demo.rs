//! Demo mode (`ON_AIR_DEMO=1`): the mock core dressed up for a screen
//! recording. It implies `ON_AIR_MOCK=1` and swaps the placeholder fixtures
//! for a believable home setup, gives speakers a short connect delay, and
//! drives the level meter from a synthetic music-like envelope while an
//! output is live. Nothing here touches hardware or the network.

use std::f32::consts::TAU;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use on_air_api::{OutputPhase, WsEvent};

use crate::patchbay::pulse::{Catalog, Device};
use crate::sender::airplay_mdns::CatalogDevice;
use crate::sender::bluetooth::MockBluetoothAdapter;
use crate::sender::sonos::discovery::SonosDevice;
use crate::state::CoreState;
use on_air_api::PatchbaySide;

/// How long a fake speaker takes to answer, so "starting" is visible.
pub const CONNECT_DELAY: Duration = Duration::from_millis(900);

pub fn enabled() -> bool {
    std::env::var("ON_AIR_DEMO").ok().as_deref() == Some("1")
}

/// The capture input, in both `/api/inputs` and the patchbay.
pub const INPUT: &str = "USB Audio Capture";

/// Replace the mock fixtures on a fresh mock state: one USB capture input
/// and two speakers, an AirPlay stereo pair and a Sonos.
pub async fn dress(state: &mut CoreState) {
    state.input().set_mock_inputs(vec![INPUT.into()]);
    *state.airplay.lock() = vec![CatalogDevice {
        id: "ap-bedroom".into(),
        name: "Bedroom HomePods".into(),
        needs_pair: false,
        paired: true,
        kind: "pair",
        member_count: 2,
        address: String::new(),
    }];
    state.bluetooth = Arc::new(MockBluetoothAdapter::with_devices(vec![]));
    let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 11));
    let mut sonos = state.sonos.lock().await;
    *sonos = Default::default();
    sonos.upsert(
        SonosDevice::discovered(
            "uuid:demo-sonos-living-room",
            format!("http://{ip}:1400/xml/device_description.xml"),
            ip,
            "Living Room Sonos",
        ),
        Instant::now(),
    );
}

/// Local PipeWire devices the patchbay lists in demo mode.
pub fn pulse_catalog() -> Catalog {
    let device = |name: &str, label: &str, side, volume| Device {
        name: name.into(),
        label: label.into(),
        side,
        volume,
        muted: false,
        monitor_of: None,
    };
    Catalog {
        devices: vec![
            device("usb_capture", INPUT, PatchbaySide::Input, 80),
            device("speakers", "Built-in speakers", PatchbaySide::Output, 55),
        ],
        default_input: "usb_capture".into(),
        default_output: "speakers".into(),
        ..Catalog::default()
    }
}

/// Emit `LevelMeter` frames at ~30 Hz while an input and an output are live.
/// The envelope is a 118 bpm kick, an off-beat hat and a slow swell, scaled
/// by the output volume, so the meter reads like music rather than noise.
pub fn spawn_meter(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let started = Instant::now();
        let mut seed: u32 = 0x9e37_79b9;
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let live = state.output().snapshot().phase == OutputPhase::Live
                && state.input().active_name().is_some();
            if !live || state.ws_tx.receiver_count() == 0 {
                continue;
            }
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let jitter = (seed % 1000) as f32 / 1000.0;
            let t = started.elapsed().as_secs_f32();
            let (rms, peak) = envelope(t, jitter, state.output_volume());
            let _ = state.ws_tx.send(WsEvent::LevelMeter { rms, peak });
        }
    })
}

fn envelope(t: f32, jitter: f32, volume: u8) -> (f32, f32) {
    let beat = (t * 118.0 / 60.0).fract();
    let kick = (-beat * 7.0).exp();
    let hat = (-((beat + 0.5).fract()) * 18.0).exp() * 0.35;
    let swell = 0.75 + 0.25 * (t * TAU / 16.0).sin();
    let gain = 0.35 + 0.65 * f32::from(volume) / 100.0;
    let body = (0.22 + 0.38 * kick + hat + 0.08 * jitter) * swell * gain;
    let rms = body.clamp(0.0, 0.92);
    let peak = (rms * (1.35 + 0.25 * jitter)).clamp(0.0, 0.99);
    (rms, peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_stays_in_range_and_moves() {
        let samples: Vec<_> = (0..300)
            .map(|i| envelope(i as f32 * 0.033, (i % 7) as f32 / 7.0, 80))
            .collect();
        assert!(samples
            .iter()
            .all(|(r, p)| (0.0..=1.0).contains(r) && p >= r && *p < 1.0));
        let (lo, hi) = samples.iter().fold((1.0f32, 0.0f32), |(lo, hi), (r, _)| {
            (lo.min(*r), hi.max(*r))
        });
        assert!(hi - lo > 0.2, "meter barely moves: {lo}..{hi}");
    }

    #[test]
    fn the_patchbay_catalog_has_a_default_on_each_side() {
        let catalog = pulse_catalog();
        assert!(catalog
            .devices
            .iter()
            .any(|d| d.name == catalog.default_input));
        assert!(catalog
            .devices
            .iter()
            .any(|d| d.name == catalog.default_output));
    }
}
