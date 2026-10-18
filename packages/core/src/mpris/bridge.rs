//! The D-Bus side of the MPRIS bridge (Linux): the `org.mpris.MediaPlayer2`
//! interfaces, the session-bus name, and the runner that keeps them in
//! step with the core. All values come from [`super::model`], which this
//! module only marshals.
//!
//! # Activation model
//!
//! The bridge is **in-process inside `on-air-core`, gated on demand**:
//! `serve_with_state_until` starts one task beside the CD watcher and the
//! firewall watcher. While there is nothing to show — no disc in the drive
//! and no active output, exactly the condition the media widget collapses
//! under — the task parks on the core's own state-change notices and holds
//! **no D-Bus connection at all**. It connects to the session bus, serves
//! the object and owns `org.mpris.MediaPlayer2.on-air` only while that
//! face exists, and drops the connection (releasing the name) the moment
//! it stops existing. A laptop with no optical drive shows no phantom
//! player and pays nothing.
//!
//! In-process beats a spawned-on-demand daemon here: the core already runs
//! as the user service that knows all the state, so the bridge reads it
//! directly and is woken only by real changes (deck events, the input and
//! output sessions, the settings-change notice). A separate daemon would
//! have to poll the HTTP API to see anything, which is exactly the
//! polling cost this design owes nothing to, and would be one more thing
//! for the user to start and keep alive.
//!
//! # Cost
//!
//! Idle: one parked task, no bus connection, zero wakeups until a state
//! change. Active: one bus connection and one wake per state change (the
//! deck's ~1 Hz tick is the fastest). The bridge deliberately does not
//! subscribe to `CoreState::ws_tx`: that channel's receiver count is what
//! makes the pipeline compute level meters at chunk rate, and MPRIS has no
//! need for levels.
//!
//! # Crate
//!
//! zbus 6, pure Rust and pure async, with only its `service` and `tokio`
//! features: no C libraries, no second runtime (it runs on the core's
//! tokio), no `async-io`. It is built for Linux only, like MPRIS itself.
//!
//! # Signals
//!
//! The runner is the single emitter of `PropertiesChanged` and `Seeked`:
//! every property is annotated `emits_changed_signal = "false"` so no
//! setter auto-emits behind its back, and each state change leaves as one
//! batched `PropertiesChanged` whose values come from the very getters
//! `Get` serves (`zbus::as_variant::to_owned_variant`), so the two can
//! never disagree. `Position` is never notified — MPRIS forbids it —
//! `Seeked` covers its jumps.

use super::model::{self, MprisCommand, Observation};
use crate::session::OutputSnapshot;
use crate::state::CoreState;
use on_air_api::{CdStatus, WsEvent};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use zbus::object_server::SignalEmitter;
use zbus::{connection::Builder, interface, OwnedObjectPath, OwnedVariant};

/// The well-known name consumers look for.
pub const SERVICE_NAME: &str = "org.mpris.MediaPlayer2.on-air";
/// The single object every MPRIS interface lives on.
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

/// The deck's event sink buffers this many `CdStateChanged` events while
/// the runner is busy; the runner refreshes everything on overflow.
const DECK_EVENTS: usize = 16;

/// Start the bridge on the session bus as a background task. Returns the
/// handle so the embedding server can abort it at shutdown.
pub fn spawn(state: CoreState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run(state, None))
}

/// [`spawn`] against an explicit bus address. Tests run a private
/// `dbus-daemon` and pass its address here.
pub fn spawn_on(state: CoreState, address: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run(state, Some(address)))
}

/// The on-air face exists exactly while the media widget's would: a disc
/// in the drive (`GET /api/cd`) or an active output
/// (`GET /api/outputs/active`).
fn face_visible(state: &CoreState) -> bool {
    state.cd.status().present || state.output().snapshot().identity.is_some()
}

/// Live read of everything MPRIS shows, in model terms.
fn live_snapshot(state: &CoreState) -> model::MprisSnapshot {
    let capture = capture_source(state);
    model::snapshot(
        &state.cd.status(),
        state.service_enabled.load(Ordering::Acquire),
        capture.as_deref(),
        state.output_volume(),
    )
}

/// The live capture source's name, if one is capturing.
fn capture_source(state: &CoreState) -> Option<String> {
    let input = state.input().snapshot();
    if input.error.is_none() {
        input.name
    } else {
        None
    }
}

/// Monotonic milliseconds for the model's observations.
fn now_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

enum Wake {
    /// A `CdStateChanged` event: the playhead facts it carries.
    Deck {
        present: bool,
        playing: bool,
        track: u8,
        position_us: i64,
    },
    /// Something else changed; re-read everything.
    Refresh,
    /// The core is going away.
    Closed,
}

async fn wait_for_wake(
    deck: &mut broadcast::Receiver<WsEvent>,
    input: &mut tokio::sync::watch::Receiver<crate::pipeline::input::InputSnapshot>,
    output: &mut tokio::sync::watch::Receiver<OutputSnapshot>,
    changes: &mut tokio::sync::watch::Receiver<u64>,
) -> Wake {
    tokio::select! {
        event = deck.recv() => match event {
            Ok(WsEvent::CdStateChanged { present, playing, track, position_ms, .. }) => {
                Wake::Deck { present, playing, track, position_us: position_ms as i64 * 1_000 }
            }
            Ok(_) => Wake::Refresh,
            Err(broadcast::error::RecvError::Lagged(_)) => Wake::Refresh,
            Err(broadcast::error::RecvError::Closed) => Wake::Closed,
        },
        changed = input.changed() => { let _ = changed; Wake::Refresh }
        changed = output.changed() => { let _ = changed; Wake::Refresh }
        changed = changes.changed() => { let _ = changed; Wake::Refresh }
    }
}

async fn run(state: CoreState, address: Option<String>) {
    // The runner's own deck sink: one `CdStateChanged` per deck change and
    // a ~1 Hz tick while playing, and nothing at all while the deck is
    // idle. A closed sink means the deck (and so the core) is gone.
    let (deck_tx, mut deck) = broadcast::channel(DECK_EVENTS);
    state.cd.add_event_sink(deck_tx);
    let mut input = state.input().subscribe();
    let mut output = state.output().subscribe();
    let mut changes = state.subscribe_changes();
    let playhead_moved = Arc::new(AtomicBool::new(false));
    let started = std::time::Instant::now();

    loop {
        if !face_visible(&state) {
            // Gate closed: no bus connection, no name, nothing to cost.
            if matches!(wait_for_wake(&mut deck, &mut input, &mut output, &mut changes).await, Wake::Closed) {
                return;
            }
            continue;
        }
        let Some(connection) = connect(&state, address.as_deref(), &playhead_moved).await else {
            // No usable bus right now; try again on the next state change.
            if matches!(wait_for_wake(&mut deck, &mut input, &mut output, &mut changes).await, Wake::Closed) {
                return;
            }
            continue;
        };
        eprintln!("on-air mpris: owning {SERVICE_NAME}");

        // One gate cycle: serve until the face stops existing. `player`
        // and the served instance share the state; this one only marshals
        // its getters into signals, exactly as `Get` serves them.
        let player = Player {
            state: state.clone(),
            playhead_moved: playhead_moved.clone(),
        };
        let mut published = live_snapshot(&state);
        let mut last: Option<Observation> = None;
        let emitter = match SignalEmitter::new(&connection, OBJECT_PATH) {
            Ok(emitter) => emitter,
            Err(error) => {
                eprintln!("on-air mpris: no signal emitter: {error}");
                continue;
            }
        };
        loop {
            let wake = wait_for_wake(&mut deck, &mut input, &mut output, &mut changes).await;
            if matches!(wake, Wake::Closed) {
                return;
            }
            if let Wake::Deck {
                present,
                playing,
                track,
                position_us,
            } = wake
            {
                let observation =
                    Observation::new(present, track, playing, position_us, now_ms(started));
                let seeked = if playhead_moved.swap(false, Ordering::AcqRel) {
                    // A bridge `Seek`/`SetPosition`/`Next`/`Previous` just
                    // ran: the first observation after it is the new spot.
                    Some(observation.position_us)
                } else {
                    last.as_ref().and_then(|last| {
                        model::seeked_position(last, &observation, model::SEEK_TOLERANCE_US)
                    })
                };
                if let Some(position_us) = seeked {
                    if let Err(error) = emitter.seeked(position_us).await {
                        eprintln!("on-air mpris: bus gone ({error}); reconnecting");
                        break;
                    }
                }
                last = Some(observation);
            }
            let next = live_snapshot(&state);
            if let Err(error) = publish(&player, &emitter, &published, &next).await {
                eprintln!("on-air mpris: bus gone ({error}); reconnecting");
                break;
            }
            published = next;
            if !face_visible(&state) {
                break;
            }
        }
        // Dropping the connection releases the well-known name.
        drop(connection);
        eprintln!("on-air mpris: releasing {SERVICE_NAME}");
    }
}

/// Connect, register both interfaces and own the name. `None` when the
/// bus is unreachable or the name is taken: the runner tries again on the
/// next state change rather than spinning.
async fn connect(
    state: &CoreState,
    address: Option<&str>,
    playhead_moved: &Arc<AtomicBool>,
) -> Option<zbus::Connection> {
    let builder = match address {
        Some(address) => Builder::address(address),
        None => Builder::session(),
    };
    let player = Player {
        state: state.clone(),
        playhead_moved: playhead_moved.clone(),
    };
    match builder
        .name(SERVICE_NAME)
        .serve_at(OBJECT_PATH, Root)
        .serve_at(OBJECT_PATH, player)
        .build()
        .await
    {
        Ok(connection) => Some(connection),
        Err(error) => {
            eprintln!("on-air mpris: cannot own {SERVICE_NAME}: {error}");
            None
        }
    }
}

/// A getter's value, marshalled the way `Get` serves it.
fn value_of<T: serde::Serialize + zbus::Type>(value: &T) -> zbus::Result<OwnedVariant> {
    zbus::as_variant::to_owned_variant(value)
}

/// One `PropertiesChanged` per state change, values included, for exactly
/// the properties that changed. `Position` is deliberately absent: MPRIS
/// forbids notifying it and `Seeked` covers its jumps.
async fn publish(
    player: &Player,
    emitter: &SignalEmitter<'_>,
    before: &model::MprisSnapshot,
    after: &model::MprisSnapshot,
) -> zbus::Result<()> {
    let mut changed: HashMap<&str, OwnedVariant> = HashMap::new();
    if before.playback != after.playback {
        changed.insert("PlaybackStatus", value_of(&player.playback_status())?);
    }
    if before.metadata != after.metadata {
        changed.insert("Metadata", value_of(&player.metadata_property())?);
    }
    if before.volume != after.volume {
        changed.insert("Volume", value_of(&player.volume_property())?);
    }
    if before.can_go_next != after.can_go_next {
        changed.insert("CanGoNext", value_of(&player.can_go_next())?);
    }
    if before.can_go_previous != after.can_go_previous {
        changed.insert("CanGoPrevious", value_of(&player.can_go_previous())?);
    }
    if before.can_play != after.can_play {
        changed.insert("CanPlay", value_of(&player.can_play())?);
    }
    if before.can_pause != after.can_pause {
        changed.insert("CanPause", value_of(&player.can_pause())?);
    }
    if before.can_seek != after.can_seek {
        changed.insert("CanSeek", value_of(&player.can_seek())?);
    }
    if changed.is_empty() {
        return Ok(());
    }
    emitter
        .emit(
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                zbus::names::InterfaceName::from_static_str_unchecked(PLAYER_INTERFACE),
                changed,
                std::borrow::Cow::<[&str]>::Borrowed(&[]),
            ),
        )
        .await
}

/// `Raise` opens or focuses the TUI: Omarchy's own helper where it exists
/// (`omarchy-launch-or-focus-tui on-air`, the command every shipped
/// snippet binds), else `xdg-terminal-exec -e on-air`, the launcher the
/// desktop file's `Exec` line uses. Both are session tools; neither is a
/// dependency of the core — with neither on `PATH`, `Raise` reports why.
fn raise_tui() {
    const LAUNCHERS: &[(&str, &[&str])] = &[
        ("omarchy-launch-or-focus-tui", &["on-air"]),
        ("xdg-terminal-exec", &["-e", "on-air"]),
    ];
    for (program, args) in LAUNCHERS {
        let attempt = tokio::process::Command::new(program)
            .args(*args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match attempt {
            Ok(_child) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!("on-air mpris: Raise could not run {program}: {error}");
                return;
            }
        }
    }
    eprintln!("on-air mpris: Raise found no launcher (omarchy-launch-or-focus-tui, xdg-terminal-exec)");
}

fn fdo_error(error: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}

/// `org.mpris.MediaPlayer2`, the root of the object. Nothing on it ever
/// changes, so its properties are annotated `const`.
struct Root;

#[interface(name = "org.mpris.MediaPlayer2", introspection_docs = false)]
impl Root {
    /// Open or focus the on-air TUI.
    async fn raise(&self) -> zbus::fdo::Result<()> {
        raise_tui();
        Ok(())
    }

    /// `CanQuit` is false: quitting is the user service's business
    /// (`systemctl --user stop on-air-core`), so this has no effect.
    async fn quit(&self) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "CanQuit")]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "CanRaise")]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property(emits_changed_signal = "const"), name = "DesktopEntry")]
    fn desktop_entry(&self) -> &str {
        "on-air"
    }

    #[zbus(property(emits_changed_signal = "const"), name = "HasTrackList")]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Identity")]
    fn identity(&self) -> &str {
        "on-air"
    }

    #[zbus(property(emits_changed_signal = "const"), name = "SupportedMimeTypes")]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "SupportedUriSchemes")]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
}

/// `org.mpris.MediaPlayer2.Player`, the transport. Methods run in the
/// order they arrive (`spawn = false`): a transport whose `Next` can
/// overtake its `PlayPause` is a transport that double-steps tracks.
struct Player {
    state: CoreState,
    /// Set just before a playhead command is dispatched so the runner's
    /// next deck observation is published as a `Seeked` signal.
    playhead_moved: Arc<AtomicBool>,
}

impl Player {
    fn deck(&self) -> CdStatus {
        self.state.cd.status()
    }

    async fn command(&self, command: MprisCommand) -> zbus::fdo::Result<()> {
        let before = self.deck();
        let Some(request) = model::plan(command, &before) else {
            return Ok(()); // spec-mandated no-op
        };
        if command.moves_playhead() {
            self.playhead_moved.store(true, Ordering::Release);
        }
        // The same domain path `POST /api/cd/control` takes.
        let after = crate::cd::control(&self.state, &request)
            .await
            .map_err(fdo_error)?;
        if (after.track, after.position_ms) == (before.track, before.position_ms) {
            // Nothing actually moved (Next on the last track): no Seeked.
            self.playhead_moved.store(false, Ordering::Release);
        }
        Ok(())
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player", spawn = false, introspection_docs = false)]
impl Player {
    async fn next(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Next).await
    }

    async fn pause(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Pause).await
    }

    async fn play(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Play).await
    }

    async fn play_pause(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::PlayPause).await
    }

    async fn previous(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Previous).await
    }

    async fn seek(&self, offset_us: i64) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Seek { offset_us }).await
    }

    async fn set_position(
        &self,
        track_id: OwnedObjectPath,
        position_us: i64,
    ) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::SetPosition {
            track: model::parse_track_path(track_id.as_str()),
            position_us,
        })
        .await
    }

    async fn stop(&self) -> zbus::fdo::Result<()> {
        self.command(MprisCommand::Stop).await
    }

    /// The playback rate. Fixed at 1: the DSP runs at the source's rate
    /// and nothing time-stretches. Sets are accepted and ignored, as the
    /// bounds allow — the rate cannot leave `MinimumRate`..`MaximumRate`.
    #[zbus(property(emits_changed_signal = "const"), name = "Rate")]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property, name = "Rate")]
    async fn set_rate(&self, _rate: f64) {}

    #[zbus(property(emits_changed_signal = "const"), name = "MinimumRate")]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property(emits_changed_signal = "const"), name = "MaximumRate")]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    /// `"Playing"` while the service is enabled and a source is live,
    /// `"Paused"` while the service is paused, `"Stopped"` otherwise —
    /// see [`super::model::playback_status`].
    #[zbus(property(emits_changed_signal = "false"), name = "PlaybackStatus")]
    fn playback_status(&self) -> &'static str {
        model::playback_status(
            self.state.service_enabled.load(Ordering::Acquire),
            model::source_live(&self.deck(), capture_source(&self.state).as_deref()),
        )
        .as_str()
    }

    /// From `GET /api/cd`; fields with no source data are omitted rather
    /// than faked, and `mpris:artUrl` never appears because no cover-art
    /// source exists (see [`super::model::MprisMetadata`]).
    #[zbus(property(emits_changed_signal = "false"), name = "Metadata")]
    fn metadata_property(&self) -> HashMap<String, OwnedVariant> {
        let metadata = model::MprisMetadata::from_cd(&self.deck());
        let mut map = HashMap::new();
        if let Some(track_id) = metadata.track_id {
            // A per-track object path id, `o` on the wire.
            let path = zbus::ObjectPath::try_from(track_id)
                .expect("track_path builds valid object paths");
            map.insert("mpris:trackid".to_string(), OwnedVariant::from(path));
        }
        if let Some(title) = metadata.title {
            map.insert(
                "xesam:title".to_string(),
                OwnedVariant::from(zbus::Str::from(title)),
            );
        }
        if let Some(album) = metadata.album {
            map.insert(
                "xesam:album".to_string(),
                OwnedVariant::from(zbus::Str::from(album)),
            );
        }
        if let Some(track_number) = metadata.track_number {
            map.insert(
                "xesam:trackNumber".to_string(),
                OwnedVariant::from(track_number),
            );
        }
        if let Some(length_us) = metadata.length_us {
            map.insert("mpris:length".to_string(), OwnedVariant::from(length_us));
        }
        map
    }

    /// Deck position in microseconds, always live. Not notified through
    /// `PropertiesChanged` (MPRIS forbids it); `Seeked` marks the jumps.
    #[zbus(property(emits_changed_signal = "false"), name = "Position")]
    fn position(&self) -> i64 {
        self.deck().position_ms as i64 * 1_000
    }

    /// 0.0..=1.0 of the active output's volume
    /// (`GET`/`POST /api/outputs/active/volume`). With no active output
    /// the setter fails the way the HTTP endpoint does.
    #[zbus(property(emits_changed_signal = "false"), name = "Volume")]
    fn volume_property(&self) -> f64 {
        model::volume_mpris(self.state.output_volume())
    }

    #[zbus(property, name = "Volume")]
    async fn set_volume(&self, volume: f64) -> zbus::fdo::Result<()> {
        if !volume.is_finite() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "volume must be a finite number".into(),
            ));
        }
        crate::session::set_volume(&self.state, model::volume_percent(volume))
            .await
            .map_err(fdo_error)
    }

    #[zbus(property(emits_changed_signal = "const"), name = "CanControl")]
    fn can_control(&self) -> bool {
        true
    }

    /// True exactly while a disc is present.
    #[zbus(property(emits_changed_signal = "false"), name = "CanGoNext")]
    fn can_go_next(&self) -> bool {
        self.deck().present
    }

    #[zbus(property(emits_changed_signal = "false"), name = "CanGoPrevious")]
    fn can_go_previous(&self) -> bool {
        self.deck().present
    }

    #[zbus(property(emits_changed_signal = "false"), name = "CanPause")]
    fn can_pause(&self) -> bool {
        self.deck().present
    }

    #[zbus(property(emits_changed_signal = "false"), name = "CanPlay")]
    fn can_play(&self) -> bool {
        self.deck().present
    }

    #[zbus(property(emits_changed_signal = "false"), name = "CanSeek")]
    fn can_seek(&self) -> bool {
        self.deck().present
    }

    /// The playhead moved where no clock could have followed it.
    #[zbus(signal)]
    async fn seeked(signal_emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}
