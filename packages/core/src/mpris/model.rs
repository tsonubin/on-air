//! Pure MPRIS mappings: on-air state in, MPRIS values out; MPRIS commands
//! in, `POST /api/cd/control` bodies out. No D-Bus types here, so every
//! mapping the interface contract (docs/omarchy.md "Media") demands is
//! testable without a bus; [`super::bridge`] only marshals these values.
//!
//! Mapping table, MPRIS property -> on-air source:
//!
//! | MPRIS | on-air source (`GET`/`POST`) |
//! |---|---|
//! | `PlaybackStatus` | `service_enabled` (`PUT /api/service`) + source liveness (`GET /api/cd` `playing`, live capture) |
//! | `Metadata` | `GET /api/cd` (`title`, `album`, `track`, `duration_ms`) |
//! | `mpris:trackid` | `/org/on_air/track/<track>` per track number |
//! | `mpris:length` | `duration_ms` × 1000 (µs) |
//! | `mpris:artUrl` | never: no cover-art source exists in the pipeline |
//! | `Position` | `position_ms` × 1000 (µs) from the deck |
//! | `Seeked` | deck discontinuities (seek/track change) |
//! | `Volume` | `GET`/`POST /api/outputs/active/volume` (0..=100 ⇄ 0..=1) |
//! | `Rate`, `MinimumRate`, `MaximumRate` | fixed 1: no time-stretching |
//! | `CanGoNext`..`CanSeek` | `GET /api/cd` `present` |
//! | `CanControl` | constant true |
//! | `Play`/`Pause`/`PlayPause`/`Stop`/`Next`/`Previous`/`Seek`/`SetPosition` | `POST /api/cd/control` (`play`, `pause`, `next`, `prev`, `seek`) |
//! | `Raise` | launches/focuses the TUI (see [`super::bridge`]) |
//! | `Quit` | no-op: `CanQuit` is false |

use on_air_api::{CdAction, CdControlRequest, CdStatus, AUDIO_CD_INPUT};

/// MPRIS `org.mpris.MediaPlayer2.Player.PlaybackStatus`, as the contract
/// maps it onto the service flag and the live sources:
///
/// - `"Playing"` while the service is enabled and a source is live,
/// - `"Paused"` while the service is paused (`PUT /api/service`),
/// - `"Stopped"` otherwise.
///
/// "A source is live" is the deck producing (`GET /api/cd` `playing`) or a
/// non-CD capture running: the Audio CD capture keeps running while the
/// deck is paused, so counting it would report a playing disc that is in
/// fact stopped. A paused deck with the service on is therefore the
/// contract's "Stopped otherwise"; on-air's other surfaces call that state
/// "paused" at the *transport* level, which MPRIS has no room for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Stopped => "Stopped",
        }
    }
}

/// Whether the pipeline has a source producing, per [`PlaybackStatus`].
pub fn source_live(cd: &CdStatus, capture: Option<&str>) -> bool {
    cd.playing || capture.is_some_and(|name| name != AUDIO_CD_INPUT)
}

/// The three clauses of the contract, in its order.
pub fn playback_status(service_enabled: bool, source_live: bool) -> PlaybackStatus {
    if service_enabled && source_live {
        PlaybackStatus::Playing
    } else if !service_enabled {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Stopped
    }
}

/// `/org/on_air/track/<n>`: a stable, valid D-Bus object path per track
/// number (no hyphens — object path elements are `[A-Za-z0-9_]`).
pub fn track_path(track: u8) -> String {
    format!("/org/on_air/track/{track}")
}

/// The MPRIS "no track" id.
pub const NO_TRACK_PATH: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// What a `mpris:trackid` on the wire refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackRef {
    /// [`NO_TRACK_PATH`]: "the current track, whatever it is".
    NoTrack,
    /// One of [`track_path`]'s ids.
    Track(u8),
    /// Any other object path: not this player's.
    Foreign,
}

pub fn parse_track_path(path: &str) -> TrackRef {
    if path == NO_TRACK_PATH {
        return TrackRef::NoTrack;
    }
    match path.strip_prefix("/org/on_air/track/") {
        Some(rest) => match rest.parse::<u8>() {
            Ok(track) if track_path(track) == path => TrackRef::Track(track),
            _ => TrackRef::Foreign,
        },
        None => TrackRef::Foreign,
    }
}

/// `org.mpris.MediaPlayer2.Player.Metadata` for the current track.
///
/// Every field is `None` while no track is loaded, and absent source data
/// stays absent (`xesam:title` is omitted when the disc has no CD-TEXT
/// title) — nothing is filled with placeholders. `mpris:artUrl` is not
/// part of this struct on purpose: `GET /api/cd` has no art field and the
/// CD pipeline extracts no cover art, so the key is never emitted; the
/// contract allows it only "if cover art exists". When an art source
/// lands, it becomes a field here and one line in [`MprisMetadata`]
/// building.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisMetadata {
    pub track_id: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<i32>,
    pub length_us: Option<i64>,
}

impl MprisMetadata {
    /// From the body of `GET /api/cd`.
    pub fn from_cd(cd: &CdStatus) -> Self {
        if cd.track == 0 {
            return MprisMetadata::default();
        }
        MprisMetadata {
            track_id: Some(track_path(cd.track)),
            title: cd.title.clone(),
            album: cd.album.clone(),
            track_number: Some(i32::from(cd.track)),
            length_us: Some(cd.duration_ms as i64 * 1_000),
        }
    }
}

/// Everything the player face shows, mapped from one live read of the core.
#[derive(Debug, Clone, PartialEq)]
pub struct MprisSnapshot {
    pub playback: PlaybackStatus,
    pub metadata: MprisMetadata,
    pub position_us: i64,
    pub volume: f64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
}

/// `cd`: body of `GET /api/cd`. `service_enabled`: the `PUT /api/service`
/// flag. `capture`: the live capture source's name, if one is capturing.
/// `volume_percent`: `GET /api/outputs/active/volume`.
pub fn snapshot(
    cd: &CdStatus,
    service_enabled: bool,
    capture: Option<&str>,
    volume_percent: u8,
) -> MprisSnapshot {
    let disc = cd.present;
    MprisSnapshot {
        playback: playback_status(service_enabled, source_live(cd, capture)),
        metadata: MprisMetadata::from_cd(cd),
        position_us: cd.position_ms as i64 * 1_000,
        volume: volume_mpris(volume_percent),
        can_go_next: disc,
        can_go_previous: disc,
        can_play: disc,
        can_pause: disc,
        can_seek: disc,
    }
}

/// Core volume is 0..=100 percent (`OutputVolumeResponse`); MPRIS `Volume`
/// is 0.0..=1.0. Out-of-range and non-finite MPRIS values clamp to it.
pub fn volume_mpris(percent: u8) -> f64 {
    f64::from(percent) / 100.0
}

pub fn volume_percent(volume: f64) -> u8 {
    let volume = if volume.is_finite() { volume } else { 0.0 };
    (volume * 100.0).round().clamp(0.0, 100.0) as u8
}

/// An MPRIS `Player` method call, after the wire types are stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Seek { offset_us: i64 },
    SetPosition { track: TrackRef, position_us: i64 },
}

impl MprisCommand {
    /// True when the command can move the playhead, so the `Seeked` signal
    /// must follow it.
    pub fn moves_playhead(self) -> bool {
        matches!(
            self,
            MprisCommand::Next
                | MprisCommand::Previous
                | MprisCommand::Seek { .. }
                | MprisCommand::SetPosition { .. }
        )
    }
}

/// The `POST /api/cd/control` body an MPRIS method translates to; `None`
/// is a spec-mandated no-op (`SetPosition` for another track or a negative
/// position, which must do nothing).
///
/// `Stop` maps to `pause`: the deck pauses in place, the only stop
/// `CdAction` has. `PlayPause` picks from the deck's own playing flag,
/// exactly like the TUI's space bar. `Seek` and `SetPosition` arrive in
/// microseconds on the MPRIS wire and become `position_ms` here.
pub fn plan(command: MprisCommand, cd: &CdStatus) -> Option<CdControlRequest> {
    let action = |action| {
        Some(CdControlRequest {
            action,
            position_ms: None,
            track: None,
        })
    };
    let seek_to = |position_us: i64| {
        Some(CdControlRequest {
            action: CdAction::Seek,
            position_ms: Some((position_us / 1_000).max(0) as u64),
            track: None,
        })
    };
    match command {
        MprisCommand::Play => action(CdAction::Play),
        MprisCommand::Pause => action(CdAction::Pause),
        MprisCommand::PlayPause => {
            if cd.playing {
                action(CdAction::Pause)
            } else {
                action(CdAction::Play)
            }
        }
        MprisCommand::Stop => action(CdAction::Pause),
        MprisCommand::Next => action(CdAction::Next),
        MprisCommand::Previous => action(CdAction::Prev),
        MprisCommand::Seek { offset_us } => {
            let length_us = cd.duration_ms as i64 * 1_000;
            let target = (cd.position_ms as i64 * 1_000 + offset_us).clamp(0, length_us);
            seek_to(target)
        }
        MprisCommand::SetPosition { track, position_us } => {
            if position_us < 0 {
                return None;
            }
            match track {
                TrackRef::NoTrack => {}
                TrackRef::Track(track) if track == cd.track => {}
                _ => return None,
            }
            seek_to(position_us)
        }
    }
}

/// One deck observation, taken from a `CdStateChanged` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub present: bool,
    pub track: u8,
    pub playing: bool,
    pub position_us: i64,
    /// Monotonic milliseconds when this was observed.
    pub at_ms: u64,
}

impl Observation {
    pub fn new(present: bool, track: u8, playing: bool, position_us: i64, at_ms: u64) -> Self {
        Observation {
            present,
            track,
            playing,
            position_us,
            at_ms,
        }
    }
}

/// How far a playing deck may drift from the wall-clock expectation before
/// it counts as a seek. The deck ticks once a second and paces itself with
/// sleeps, so a tick can land late; the expectation chains from the last
/// observation, so only one tick of scheduling jitter is in question.
pub const SEEK_TOLERANCE_US: i64 = 1_000_000;

/// The `Seeked` position when `new` is not continuous with `prev` — the
/// deck moved where a consumer's clock could not have followed it. Media
/// appearing or vanishing is not a seek, so observations across an insert
/// or eject report `None`.
pub fn seeked_position(prev: &Observation, new: &Observation, tolerance_us: i64) -> Option<i64> {
    if !prev.present || !new.present {
        return None;
    }
    if prev.track != new.track {
        return Some(new.position_us);
    }
    if !prev.playing {
        // Nothing may move while paused: any change is a seek.
        return (new.position_us != prev.position_us).then_some(new.position_us);
    }
    let elapsed_us = (new.at_ms.saturating_sub(prev.at_ms)) as i64 * 1_000;
    let expected = prev.position_us + elapsed_us;
    ((new.position_us - expected).abs() > tolerance_us).then_some(new.position_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> CdStatus {
        // The shape `GET /api/cd` serves: an inserted disc at track 2.
        serde_json::from_value(serde_json::json!({
            "present": true,
            "playing": true,
            "track": 2,
            "track_count": 3,
            "title": "Freddie Freeloader",
            "album": "Kind of Blue",
            "position_ms": 45_500,
            "duration_ms": 568_000,
            "tracks": [
                {"number": 1, "title": "So What", "duration_ms": 562_000},
                {"number": 2, "title": "Freddie Freeloader", "duration_ms": 568_000},
                {"number": 3, "duration_ms": 544_000}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn playback_status_follows_enabled_paused_and_empty() {
        // Service enabled and a source is live.
        assert_eq!(
            playback_status(true, true),
            PlaybackStatus::Playing,
            "enabled with a live source plays"
        );
        // The service is paused via `PUT /api/service`.
        assert_eq!(
            playback_status(false, true),
            PlaybackStatus::Paused,
            "a paused service is Paused even mid-track"
        );
        assert_eq!(
            playback_status(false, false),
            PlaybackStatus::Paused,
            "the service flag is the paused clause"
        );
        // Otherwise.
        assert_eq!(
            playback_status(true, false),
            PlaybackStatus::Stopped,
            "enabled with nothing producing is Stopped"
        );
        assert_eq!(PlaybackStatus::Playing.as_str(), "Playing");
        assert_eq!(PlaybackStatus::Paused.as_str(), "Paused");
        assert_eq!(PlaybackStatus::Stopped.as_str(), "Stopped");
    }

    #[test]
    fn a_source_is_live_for_the_deck_or_a_non_cd_capture() {
        let mut cd = disc();
        assert!(source_live(&cd, Some("Audio CD")), "deck playing is live");
        assert!(source_live(&cd, Some("System Audio")), "capture is live");
        assert!(source_live(&cd, None), "the deck alone is a source");
        cd.playing = false;
        assert!(
            !source_live(&cd, Some("Audio CD")),
            "a paused deck's idle capture is not a live source"
        );
        assert!(source_live(&cd, Some("Mic")), "a mic still is");
        assert!(!source_live(&cd, None));
    }

    #[test]
    fn a_paused_deck_with_the_service_on_reports_stopped() {
        let mut cd = disc();
        cd.playing = false;
        assert_eq!(
            snapshot(&cd, true, Some("Audio CD"), 50).playback,
            PlaybackStatus::Stopped,
            "the contract's third clause: not playing, not service-paused"
        );
    }

    #[test]
    fn metadata_maps_the_cd_response_fields() {
        let metadata = MprisMetadata::from_cd(&disc());
        assert_eq!(metadata.track_id.as_deref(), Some("/org/on_air/track/2"));
        assert_eq!(metadata.title.as_deref(), Some("Freddie Freeloader"));
        assert_eq!(metadata.album.as_deref(), Some("Kind of Blue"));
        assert_eq!(metadata.track_number, Some(2));
        assert_eq!(metadata.length_us, Some(568_000_000), "mpris:length is µs");
    }

    #[test]
    fn metadata_omits_absent_fields_and_never_invents_art() {
        // No CD-TEXT title on track 3: the field stays absent.
        let mut cd = disc();
        cd.track = 3;
        cd.title = None;
        let metadata = MprisMetadata::from_cd(&cd);
        assert_eq!(metadata.title, None);
        assert_eq!(metadata.track_id.as_deref(), Some("/org/on_air/track/3"));
        assert_eq!(metadata.track_number, Some(3));
        // Empty deck: nothing loaded, nothing claimed.
        assert_eq!(
            MprisMetadata::from_cd(&CdStatus::empty()),
            MprisMetadata::default()
        );
    }

    #[test]
    fn track_ids_are_valid_object_paths_per_track() {
        assert_eq!(track_path(1), "/org/on_air/track/1");
        assert_eq!(parse_track_path("/org/on_air/track/1"), TrackRef::Track(1));
        assert_eq!(parse_track_path(NO_TRACK_PATH), TrackRef::NoTrack);
        assert_eq!(parse_track_path("/org/on_air/track/01"), TrackRef::Foreign);
        assert_eq!(parse_track_path("/org/on_air/track/256"), TrackRef::Foreign);
        assert_eq!(parse_track_path("/org/vlc/track/1"), TrackRef::Foreign);
    }

    #[test]
    fn can_flags_flip_with_the_disc() {
        let playing = snapshot(&disc(), true, Some("Audio CD"), 50);
        assert!(playing.can_go_next && playing.can_go_previous);
        assert!(playing.can_play && playing.can_pause && playing.can_seek);
        let empty = snapshot(&CdStatus::empty(), true, None, 50);
        assert!(
            !empty.can_go_next && !empty.can_go_previous && !empty.can_play,
            "no disc, nothing to control"
        );
        assert!(!empty.can_pause && !empty.can_seek);
    }

    #[test]
    fn volume_maps_both_ways() {
        assert_eq!(volume_mpris(50), 0.5);
        assert_eq!(volume_mpris(0), 0.0);
        assert_eq!(volume_mpris(100), 1.0);
        assert_eq!(volume_percent(0.5), 50);
        assert_eq!(volume_percent(1.0), 100);
        assert_eq!(volume_percent(0.424), 42);
        assert_eq!(volume_percent(2.5), 100, "clamped to the contract's 0..=1");
        assert_eq!(volume_percent(-1.0), 0);
        assert_eq!(volume_percent(f64::NAN), 0, "total function");
    }

    #[test]
    fn transport_commands_reach_the_cd_control_wire_body() {
        let cd = disc();
        let body = |command| serde_json::to_value(plan(command, &cd).unwrap()).unwrap();
        assert_eq!(
            body(MprisCommand::Play),
            serde_json::json!({"action": "play"})
        );
        assert_eq!(
            body(MprisCommand::Pause),
            serde_json::json!({"action": "pause"})
        );
        assert_eq!(
            body(MprisCommand::Stop),
            serde_json::json!({"action": "pause"})
        );
        assert_eq!(
            body(MprisCommand::Next),
            serde_json::json!({"action": "next"})
        );
        assert_eq!(
            body(MprisCommand::Previous),
            serde_json::json!({"action": "prev"})
        );
        let mut paused = disc();
        paused.playing = false;
        assert_eq!(
            serde_json::to_value(plan(MprisCommand::PlayPause, &paused).unwrap()).unwrap(),
            serde_json::json!({"action": "play"}),
            "PlayPause presses play on a stopped deck"
        );
        assert_eq!(
            serde_json::to_value(plan(MprisCommand::PlayPause, &cd).unwrap()).unwrap(),
            serde_json::json!({"action": "pause"}),
            "PlayPause presses pause on a playing deck"
        );
    }

    #[test]
    fn seek_is_relative_and_clamped_to_the_track() {
        let cd = disc(); // position 45_500 ms, length 568_000 ms
        let seek = |offset_us| {
            serde_json::to_value(plan(MprisCommand::Seek { offset_us }, &cd).unwrap()).unwrap()
        };
        assert_eq!(
            seek(30_000_000),
            serde_json::json!({"action": "seek", "position_ms": 75_500})
        );
        assert_eq!(
            seek(-5_500_000),
            serde_json::json!({"action": "seek", "position_ms": 40_000})
        );
        assert_eq!(
            seek(-50_000_000),
            serde_json::json!({"action": "seek", "position_ms": 0}),
            "a seek before the track lands at zero"
        );
        assert_eq!(
            seek(999_000_000),
            serde_json::json!({"action": "seek", "position_ms": 568_000}),
            "a seek past the end lands at the end"
        );
    }

    #[test]
    fn set_position_only_seeks_the_current_track() {
        let cd = disc(); // track 2
        let set = |track, position_us| plan(MprisCommand::SetPosition { track, position_us }, &cd);
        assert_eq!(
            serde_json::to_value(set(TrackRef::Track(2), 61_000_000).unwrap()).unwrap(),
            serde_json::json!({"action": "seek", "position_ms": 61_000}),
            "the current track seeks to the absolute position"
        );
        assert_eq!(
            serde_json::to_value(set(TrackRef::NoTrack, 61_000_000).unwrap()).unwrap(),
            serde_json::json!({"action": "seek", "position_ms": 61_000}),
            "NoTrack means the current track"
        );
        assert_eq!(
            set(TrackRef::Track(1), 61_000_000),
            None,
            "another track is a no-op, per MPRIS"
        );
        assert_eq!(set(TrackRef::Foreign, 61_000_000), None);
        assert_eq!(
            set(TrackRef::NoTrack, -1),
            None,
            "a negative position is a no-op"
        );
    }

    #[test]
    fn commands_that_move_the_playhead_are_the_seek_family() {
        assert!(MprisCommand::Next.moves_playhead());
        assert!(MprisCommand::Previous.moves_playhead());
        assert!(MprisCommand::Seek { offset_us: 1 }.moves_playhead());
        assert!(MprisCommand::SetPosition {
            track: TrackRef::NoTrack,
            position_us: 0
        }
        .moves_playhead());
        assert!(!MprisCommand::Play.moves_playhead());
        assert!(!MprisCommand::Pause.moves_playhead());
        assert!(!MprisCommand::PlayPause.moves_playhead());
        assert!(!MprisCommand::Stop.moves_playhead());
    }

    fn obs(track: u8, playing: bool, position_ms: i64, at_ms: u64) -> Observation {
        Observation::new(true, track, playing, position_ms * 1_000, at_ms)
    }

    #[test]
    fn paused_movement_is_always_a_seek() {
        let prev = obs(1, false, 10_000, 1_000);
        assert_eq!(
            seeked_position(&prev, &obs(1, false, 40_000, 1_005), SEEK_TOLERANCE_US),
            Some(40_000_000),
            "even a small seek while paused is exact"
        );
        assert_eq!(
            seeked_position(&prev, &obs(1, false, 10_000, 1_005), SEEK_TOLERANCE_US),
            None,
            "a paused deck that does not move is continuous"
        );
    }

    #[test]
    fn a_playing_deck_follows_the_clock_unless_it_jumps() {
        // One tick, 1 s apart, position advanced 1 s: continuous.
        let prev = obs(1, true, 10_000, 1_000);
        assert_eq!(
            seeked_position(&prev, &obs(1, true, 11_000, 2_000), SEEK_TOLERANCE_US),
            None
        );
        // A late tick with matching position drift is still continuous.
        assert_eq!(
            seeked_position(&prev, &obs(1, true, 11_300, 2_350), SEEK_TOLERANCE_US),
            None
        );
        // A 30 s jump while playing is a seek.
        assert_eq!(
            seeked_position(&prev, &obs(1, true, 40_000, 2_000), SEEK_TOLERANCE_US),
            Some(40_000_000)
        );
        // Backwards is a seek however small.
        assert_eq!(
            seeked_position(&prev, &obs(1, true, 9_500, 2_000), SEEK_TOLERANCE_US),
            Some(9_500_000)
        );
    }

    #[test]
    fn a_track_change_is_a_seek_to_the_new_position() {
        let prev = obs(1, true, 561_000, 561_000);
        assert_eq!(
            seeked_position(&prev, &obs(2, true, 0, 562_000), SEEK_TOLERANCE_US),
            Some(0),
            "auto-advance lands at the new track's start"
        );
        let paused = obs(1, false, 0, 1_000);
        assert_eq!(
            seeked_position(&paused, &obs(3, false, 0, 1_010), SEEK_TOLERANCE_US),
            Some(0),
            "goto while paused too"
        );
    }

    #[test]
    fn media_appearing_or_vanishing_is_not_a_seek() {
        let present = obs(1, true, 20_000, 1_000);
        let absent = Observation::new(false, 0, false, 0, 1_010);
        assert_eq!(seeked_position(&present, &absent, SEEK_TOLERANCE_US), None);
        assert_eq!(seeked_position(&absent, &present, SEEK_TOLERANCE_US), None);
    }

    #[test]
    fn snapshots_map_position_and_volume_from_the_deck() {
        let snapshot = snapshot(&disc(), true, Some("Audio CD"), 42);
        assert_eq!(
            snapshot.position_us, 45_500_000,
            "Position follows the deck in µs"
        );
        assert_eq!(snapshot.volume, 0.42);
    }
}
