//! The one place a sender decides how many channels it requests.
//!
//! Every live construction of a [`crate::dsp::bridge::RateBridge`],
//! [`super::resample::SteerableResampler`],
//! [`super::resample::StreamingResampler`] or [`super::resample::PcmResampler`]
//! takes a [`ChannelPlan`], and a `ChannelPlan` is only built by
//! [`SinkConstruction::plan`]: the construction site, the user's
//! [`ChannelMode`] and the destination's honest channel report. A sender
//! therefore cannot request a channel count without naming the site it
//! belongs to and the destination it plays to, and the count itself is
//! computed here — never supplied by the caller.
//!
//! Two guarantees fall out of that, and are pinned by
//! `tests/channel_construction.rs` (roadmap A6):
//!
//! - a stereo destination gets [`PIPELINE_CHANNELS`]; a request of 1 is
//!   impossible unless the destination reported one channel (a genuinely
//!   mono device) or the user explicitly chose [`ChannelMode::Mono`];
//! - `stereo` on a mono destination is rejected with an explanation
//!   ([`ChannelModeError`]), never silently collapsed (roadmap A4).

use super::PIPELINE_CHANNELS;
pub use on_air_api::ChannelMode;

/// Where a sender plays, as far as its channel count is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// Display name, for explanations.
    pub name: String,
    /// What the destination reports for its channel count. `None` means
    /// "unknown": the A4 rejection then defers to the sink, which re-checks
    /// when the real count is known.
    pub reported_channels: Option<u16>,
}

impl Destination {
    /// A destination known to play exactly one channel.
    pub fn mono(name: impl Into<String>) -> Self {
        Destination {
            name: name.into(),
            reported_channels: Some(1),
        }
    }

    /// A destination that reported `channels` channels.
    pub fn reported(name: impl Into<String>, channels: u16) -> Self {
        Destination {
            name: name.into(),
            reported_channels: Some(channels.max(1)),
        }
    }

    /// A destination whose channel count could not be probed.
    pub fn unknown(name: impl Into<String>) -> Self {
        Destination {
            name: name.into(),
            reported_channels: None,
        }
    }

    /// A stereo destination by construction: the radio WAV surface.
    pub fn stereo(name: impl Into<String>) -> Self {
        Self::reported(name, 2)
    }

    /// Only a genuinely mono device is mono.
    pub fn is_mono(&self) -> bool {
        self.reported_channels == Some(1)
    }
}

/// Why a user's channel mode was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelModeError {
    /// `stereo` on a destination that plays one channel.
    StereoOnMono { name: String },
}

impl ChannelModeError {
    /// The one explanation shown to the user: what happened and what to do.
    pub fn message(&self) -> String {
        match self {
            ChannelModeError::StereoOnMono { name } => format!(
                "{name} plays mono only; stereo would drop the right channel. Choose Auto or Mono."
            ),
        }
    }
}

impl std::fmt::Display for ChannelModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ChannelModeError {}

/// Every live construction site of a sender-side (or pipeline-side) channel
/// fit. [`Self::ALL`] is the registry `tests/channel_construction.rs`
/// walks; a new sender joins it by adding a variant here — the exhaustive
/// match in [`Self::default_channels`] refuses to compile until the site is
/// classified, and [`Self::COUNT`] pins the registry size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkConstruction {
    /// `api/stream.rs`: the radio WAV served to Sonos and AirPlay.
    RadioWav,
    /// `sender/bluetooth_pcm.rs` `PacatPcmSink`: the `pacat` pipe (Bluetooth
    /// via Pulse and every "This computer" output).
    PulsePcm,
    /// `sender/bluetooth_pcm.rs` `CpalPcmSink`: an OS audio endpoint whose
    /// stream layout the device dictates (Bluetooth via cpal).
    CpalPcm,
    /// `pipeline/mod.rs`: the capture thread's own resampler; the pipeline
    /// layout by definition.
    ProcessingThread,
    /// `audio_benchmark.rs`.
    Benchmark,
}

impl SinkConstruction {
    /// Every site. Bump [`Self::COUNT`] with the enum; the array size and
    /// the registry test both fail until they agree.
    pub const COUNT: usize = 5;
    pub const ALL: [SinkConstruction; Self::COUNT] = [
        SinkConstruction::RadioWav,
        SinkConstruction::PulsePcm,
        SinkConstruction::CpalPcm,
        SinkConstruction::ProcessingThread,
        SinkConstruction::Benchmark,
    ];

    /// The channels this site's historical (`auto`) fit requests, given
    /// what the destination reported. Sites that own a stereo wire request
    /// the pipeline layout; an OS device sink requests exactly what the
    /// device reported (so a genuinely mono device is the only mono report).
    fn default_channels(self, destination: &Destination) -> usize {
        match self {
            SinkConstruction::RadioWav
            | SinkConstruction::PulsePcm
            | SinkConstruction::ProcessingThread
            | SinkConstruction::Benchmark => PIPELINE_CHANNELS,
            SinkConstruction::CpalPcm => destination.reported_channels.map_or(
                PIPELINE_CHANNELS,
                |channels| usize::from(channels).max(1),
            ),
        }
    }

    /// The channel fit for this site, or the A4 rejection. This is the only
    /// way to build a [`ChannelPlan`].
    pub fn plan(
        self,
        mode: ChannelMode,
        destination: Destination,
    ) -> Result<ChannelPlan, ChannelModeError> {
        if mode == ChannelMode::Stereo && destination.is_mono() {
            return Err(ChannelModeError::StereoOnMono {
                name: destination.name.clone(),
            });
        }
        let channels = match mode {
            ChannelMode::Mono => 1,
            ChannelMode::Auto | ChannelMode::Stereo => self.default_channels(&destination),
        };
        Ok(ChannelPlan {
            site: self,
            mode,
            destination,
            channels,
        })
    }

    /// The historical fit: [`ChannelMode::Auto`] never refuses a
    /// destination, so this cannot fail.
    pub fn auto(self, destination: Destination) -> ChannelPlan {
        self.plan(ChannelMode::Auto, destination)
            .expect("auto never refuses a destination")
    }
}

/// One sender's justified channel request. The channel-aware DSP
/// constructors ([`crate::dsp::bridge::RateBridge`],
/// [`super::resample::SteerableResampler`],
/// [`super::resample::StreamingResampler`],
/// [`super::resample::PcmResampler`]) take one; the fields are private so no
/// caller can invent a count the registry does not justify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPlan {
    site: SinkConstruction,
    mode: ChannelMode,
    destination: Destination,
    channels: usize,
}

impl ChannelPlan {
    /// The construction site this plan belongs to.
    pub fn site(&self) -> SinkConstruction {
        self.site
    }

    /// The user's channel mode.
    pub fn mode(&self) -> ChannelMode {
        self.mode
    }

    /// The destination this plan plays to.
    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    /// The channels this construction requests. Never 0; 1 only for
    /// [`ChannelMode::Mono`] or a genuinely mono destination.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The pipeline side of the same construction: interleaved stereo by
    /// definition ([`PIPELINE_CHANNELS`]), for resampling and EQ of
    /// pipeline data before the channel fit.
    pub fn pipeline_side(&self) -> ChannelPlan {
        ChannelPlan {
            site: self.site,
            mode: self.mode,
            destination: self.destination.clone(),
            channels: PIPELINE_CHANNELS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_destination_never_loses_a_channel() {
        for site in SinkConstruction::ALL {
            for mode in ChannelMode::ALL {
                let plan = site
                    .plan(mode, Destination::stereo("Living Room"))
                    .unwrap_or_else(|error| panic!("{site:?} {mode:?}: {error}"));
                if mode != ChannelMode::Mono {
                    assert_eq!(plan.channels(), PIPELINE_CHANNELS, "{site:?} {mode:?}");
                }
            }
        }
    }

    #[test]
    fn only_a_genuinely_mono_device_or_an_explicit_mono_mode_gets_one_channel() {
        for site in SinkConstruction::ALL {
            for mode in ChannelMode::ALL {
                for reported in [1u16, 2, 4, 8] {
                    let destination = Destination::reported("Device", reported);
                    let plan = match site.plan(mode, destination) {
                        Ok(plan) => plan,
                        // The one refusal (roadmap A4), asserted in full by
                        // the rejection test below.
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
    fn stereo_on_a_mono_destination_is_rejected_with_an_explanation() {
        let error = SinkConstruction::CpalPcm
            .plan(ChannelMode::Stereo, Destination::mono("Mono Speaker"))
            .unwrap_err();
        assert_eq!(
            error.message(),
            "Mono Speaker plays mono only; stereo would drop the right channel. Choose Auto or Mono."
        );
    }

    #[test]
    fn the_registry_lists_every_site_once() {
        // Exhaustive on purpose: a new site fails to compile here until it
        // is classified (and `COUNT`/`ALL` say so too).
        fn site_id(site: SinkConstruction) -> usize {
            match site {
                SinkConstruction::RadioWav => 0,
                SinkConstruction::PulsePcm => 1,
                SinkConstruction::CpalPcm => 2,
                SinkConstruction::ProcessingThread => 3,
                SinkConstruction::Benchmark => 4,
            }
        }
        let mut ids: Vec<usize> = SinkConstruction::ALL.iter().copied().map(site_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..SinkConstruction::COUNT).collect::<Vec<_>>());
    }
}
