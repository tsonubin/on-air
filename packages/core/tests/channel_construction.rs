//! Roadmap A6: no path may silently collapse stereo to mono.
//!
//! This test walks every sender construction and asserts the channel count
//! it requests. It can do that because a sender cannot request a count at
//! all: [`RateBridge`], [`StreamingResampler`] and [`PcmResampler`] accept
//! only a [`ChannelPlan`], and a [`ChannelPlan`] is only built by
//! [`SinkConstruction::plan`] from the construction site (the registry
//! walked here), the user's [`ChannelMode`] and the destination's honest
//! report. A new sender must name a registry site — adding a variant fails
//! to compile until it is classified in the exhaustive matches — and the
//! count it gets is computed centrally, never spelled at its call site
//! (`pacat_args`, `wav_header` and `silence_chunk` take the plan too).
//!
//! So: a stereo destination gets [`PIPELINE_CHANNELS`] on every path; a
//! request of 1 is possible only for a genuinely mono device or the user's
//! explicit mono mode; and wide destinations keep their extra channels
//! silent by explicit, tested policy (`map_stereo`), never by accident.

use on_air_core::api::stream::radio_plan;
use on_air_core::dsp::bridge::{map_stereo, RateBridge};
use on_air_core::dsp::channel::{ChannelMode, Destination, SinkConstruction};
use on_air_core::dsp::resample::PcmResampler;
use on_air_core::dsp::PIPELINE_CHANNELS;

/// What each site historically (`auto`) requests for a destination that
/// reported `reported` channels. Spelled out here independently of the
/// implementation: a new construction site does not compile into this test
/// until its historical fit is stated, so no sender can enter the tree
/// classified as "requests 1 channel" by default.
fn expected_auto_channels(site: SinkConstruction, reported: Option<u16>) -> usize {
    match site {
        // Stereo-wire sites: the pipeline layout reaches the far end (or
        // Pulse) untouched, exactly as before (auto must not change what
        // existing users hear).
        SinkConstruction::RadioWav
        | SinkConstruction::PulsePcm
        | SinkConstruction::ProcessingThread
        | SinkConstruction::Benchmark => PIPELINE_CHANNELS,
        // An OS device sink requests exactly what the device reported.
        SinkConstruction::CpalPcm => reported.map_or(PIPELINE_CHANNELS, |c| usize::from(c).max(1)),
    }
}

fn destination(reported: Option<u16>) -> Destination {
    match reported {
        Some(channels) => Destination::reported("Device", channels),
        None => Destination::unknown("Device"),
    }
}

#[test]
fn every_sender_construction_asks_for_the_channel_count_its_destination_justifies() {
    for site in SinkConstruction::ALL {
        for reported in [None, Some(1u16), Some(2), Some(4), Some(8)] {
            let expected = expected_auto_channels(site, reported);
            let plan = site
                .plan(ChannelMode::Auto, destination(reported))
                .expect("auto never refuses a destination");
            assert_eq!(plan.channels(), expected, "{site:?} auto on {reported:?}");

            // The DSP each sender drives requests exactly the plan's count.
            let bridge = RateBridge::new(44_100, 48_000, &plan);
            assert_eq!(bridge.channels(), plan.channels(), "{site:?} bridge");
            let resampler = PcmResampler::new(44_100, 48_000, &plan);
            assert_eq!(resampler.channels(), plan.channels(), "{site:?} resampler");
        }
    }
}

#[test]
fn no_sender_loses_a_channel_on_a_stereo_destination() {
    for site in SinkConstruction::ALL {
        for mode in ChannelMode::ALL {
            let plan = site
                .plan(mode, Destination::stereo("Living Room"))
                .unwrap_or_else(|error| panic!("{site:?} {mode:?}: {error}"));
            if mode != ChannelMode::Mono {
                assert_eq!(
                    plan.channels(),
                    PIPELINE_CHANNELS,
                    "{site:?} {mode:?} collapsed a stereo destination"
                );
            }
        }
    }
}

#[test]
fn one_channel_is_only_for_a_genuinely_mono_device_or_an_explicit_mono_mode() {
    for site in SinkConstruction::ALL {
        for mode in ChannelMode::ALL {
            for reported in [1u16, 2, 4, 8] {
                let plan = match site.plan(mode, destination(Some(reported))) {
                    Ok(plan) => plan,
                    // The one refusal (roadmap A4), asserted in full by
                    // `stereo_on_a_genuinely_mono_device_is_rejected_with_an_explanation`.
                    Err(error) => {
                        assert_eq!(mode, ChannelMode::Stereo, "{site:?}: {error}");
                        assert_eq!(reported, 1, "{site:?}: {error}");
                        continue;
                    }
                };
                if plan.channels() == 1 {
                    assert!(
                        mode == ChannelMode::Mono || reported == 1,
                        "{site:?} {mode:?} collapsed a {reported}-channel destination to mono"
                    );
                }
            }
        }
    }
}

#[test]
fn stereo_on_a_genuinely_mono_device_is_rejected_with_an_explanation() {
    for site in SinkConstruction::ALL {
        let error = site
            .plan(ChannelMode::Stereo, Destination::mono("Mono Speaker"))
            .expect_err("stereo on a mono device must be refused, not silently collapsed");
        assert_eq!(
            error.message(),
            "Mono Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono."
        );
    }
}

#[test]
fn mono_mode_downmixes_a_stereo_destination_to_a_genuine_mono_stream() {
    for site in SinkConstruction::ALL {
        let plan = site
            .plan(ChannelMode::Mono, Destination::stereo("Living Room"))
            .unwrap();
        assert_eq!(plan.channels(), 1, "{site:?} mono");
        let mut bridge = RateBridge::new(44_100, 44_100, &plan);
        let out = bridge.process_l16(&on_air_core::dsp::bridge::f32_to_l16_le(&[
            0.5, -0.5, 0.25, 0.75,
        ]));
        // One sample per frame — a true (L + R) / 2 downmix, never a
        // duplicated mono pair.
        assert_eq!(out.len(), 2, "{site:?} mono stream is not genuinely mono");
        assert!((out[0] - 0.0).abs() < 0.01 && (out[1] - 0.5).abs() < 0.01);
    }
}

#[test]
fn the_radio_wav_follows_the_active_output_channel_mode() {
    // `auto` is byte-identical to the historical stereo stream.
    assert_eq!(radio_plan(ChannelMode::Auto).channels(), PIPELINE_CHANNELS);
    assert_eq!(radio_plan(ChannelMode::Stereo).channels(), PIPELINE_CHANNELS);
    // Mono becomes a genuine one-channel WAV.
    assert_eq!(radio_plan(ChannelMode::Mono).channels(), 1);
}

#[test]
fn wide_destinations_keep_their_extra_channels_silent_by_explicit_policy() {
    // The one wide-device policy, decided and tested: the pipeline stereo
    // content keeps its two channels and every further channel is silent —
    // content invented for them would be a lie.
    assert_eq!(map_stereo(&[0.5, 0.25], 4), vec![0.5, 0.25, 0.0, 0.0]);
    for site in SinkConstruction::ALL {
        let plan = site
            .plan(ChannelMode::Auto, Destination::reported("Surround", 4))
            .unwrap();
        assert!(
            plan.channels() == 2 || plan.channels() == 4,
            "{site:?} invented a channel layout for a wide destination"
        );
    }
}
