//! Cross-protocol playout synchronization.
//!
//! This is the foundation for mixed protocols playing in sync (roadmap item
//! 2). Three pieces cooperate:
//!
//! - a [`SyncGroup`] (see [`group`]) gives several outputs one presentation
//!   epoch and per-output delay lines, so fast transports wait for slow
//!   ones. Groups are named user configuration: several may exist at once,
//!   an output is a member of at most one at a time, and membership only
//!   takes effect at the group's next scheduled start;
//! - [`latency`] supplies the per-transport latency model: documented,
//!   explicitly unmeasured defaults plus a user trim in milliseconds;
//! - [`drift`] measures each consumer's real consumption rate against the
//!   timeline and steers a variable-ratio resampler (drift correction).
//!
//! The rule the module encodes: a sync group is *more* constrained than two
//! unrelated outputs. Two outputs outside any group free-run and drift
//! apart — fine and expected. Two outputs inside a group share an epoch and
//! are held to their delay lines and drift steering. Simultaneous
//! multi-output is first-class through sync groups of sessions; no two
//! sessions may drive the same receiver, which stays a hard refusal.

pub mod drift;
pub mod group;
pub mod latency;
pub mod lifecycle;

pub use drift::{DriftController, DriftReport};
pub use group::{delay_frames, MemberId, OutputKey, PresentationEpoch, StartPlan, SyncGroup};
pub use latency::{
    default_latency_ms, effective_latency, effective_latency_ms, effective_latency_us,
    LatencyEstimate,
};
pub use lifecycle::GroupError;

use crate::session::OutputSession;
use crate::settings::{SavedSyncGroup, SavedSyncGroupMember};
use on_air_api::{
    CalibrationMeasurement, OutputPhase, OutputSyncStatus, SyncGroupMember, SyncGroupView,
    SyncState,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Rate assumed for outputs whose feed rate was never recorded, only used to
/// size a delay line in a status row.
const FALLBACK_RATE_HZ: u32 = 48_000;

/// Per-output state the registry remembers: name, trim, the acoustic
/// measurement (if one has been applied), the rate its feed runs at, and
/// the last drift report its feed made.
#[derive(Default)]
struct OutputRow {
    name: String,
    trim_ms: i32,
    measured: Option<CalibrationMeasurement>,
    rate_hz: Option<u32>,
    report: Option<DriftReport>,
}

/// One named sync group and the sessions its `start` gave its members.
struct GroupRow {
    name: String,
    group: Arc<SyncGroup>,
}

#[derive(Default)]
struct Registry {
    groups: Vec<GroupRow>,
    next_group_id: u64,
    membership: HashMap<OutputKey, (Arc<SyncGroup>, MemberId)>,
    /// Member sessions a group's `start` created. An entry claims the
    /// receiver until the member is stopped or leaves its group.
    playing: HashMap<OutputKey, OutputSession>,
    outputs: HashMap<OutputKey, OutputRow>,
}

/// Why [`SyncController::join`] refused a membership change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    /// The output is already a member of another sync group. An output
    /// plays in at most one group at a time; take it out of the other group
    /// first.
    #[error("this output already plays in another sync group")]
    AlreadyInGroup,
}

/// Process-wide sync registry: sync groups, per-output latency trims and the
/// drift reports the feeds make. Held by [`crate::state::CoreState`] and
/// shared with its patchbay children so a group can span sessions.
pub struct SyncController {
    registry: Mutex<Registry>,
}

impl SyncController {
    pub fn new() -> Self {
        SyncController {
            registry: Mutex::new(Registry::default()),
        }
    }

    /// A fresh, empty named sync group. Outputs join it via
    /// [`SyncController::join`].
    pub fn create_group(self: &Arc<Self>, name: impl Into<String>) -> Arc<SyncGroup> {
        let mut registry = self.registry.lock();
        let group = Arc::new(SyncGroup::new(registry.next_group_id));
        registry.next_group_id += 1;
        registry.groups.push(GroupRow {
            name: name.into(),
            group: group.clone(),
        });
        group
    }

    /// Join `output` to `group` at its effective latency (the applied
    /// measurement when one exists, else the documented default, plus the
    /// user's trim). An output plays in at most one group at a time:
    /// joining is REFUSED while it is a member of another group, so two
    /// groups can never pull the same speaker in different directions.
    /// Re-joining the same group is idempotent.
    pub fn join(
        self: &Arc<Self>,
        group: &Arc<SyncGroup>,
        output: &on_air_api::ActiveOutput,
    ) -> Result<MemberId, JoinError> {
        let key = OutputKey::new(&output.transport, &output.device_id);
        let latency = {
            let mut registry = self.registry.lock();
            if let Some((current, member)) = registry.membership.get(&key) {
                return if current.id() == group.id() {
                    Ok(*member)
                } else {
                    Err(JoinError::AlreadyInGroup)
                };
            }
            let row = registry.outputs.entry(key.clone()).or_default();
            row.name = output.device_name.clone();
            let (trim_ms, measured) = (row.trim_ms, row.measured.as_ref().map(|m| m.latency_us));
            latency::effective_latency(&output.transport, measured, trim_ms)
        };
        let member = group.join(key.clone(), latency);
        self.registry
            .lock()
            .membership
            .insert(key, (group.clone(), member));
        Ok(member)
    }

    /// Remove one output from its group, if any.
    pub fn leave(&self, key: &OutputKey) -> Option<(Arc<SyncGroup>, MemberId)> {
        let mut registry = self.registry.lock();
        let entry = registry.membership.remove(key)?;
        entry.0.leave(entry.1);
        Some(entry)
    }

    /// Forget a whole group: every output leaves it. Any playing member
    /// sessions must be stopped by the caller first
    /// ([`SyncController::take_playing`]).
    pub fn dissolve_group(&self, group_id: u64) -> Option<Arc<SyncGroup>> {
        let mut registry = self.registry.lock();
        let row = registry
            .groups
            .iter()
            .position(|row| row.group.id() == group_id)?;
        let row = registry.groups.remove(row);
        registry
            .membership
            .retain(|_, (group, _)| group.id() != group_id);
        Some(row.group)
    }

    /// The group one output belongs to, if any.
    pub fn group_of(&self, key: &OutputKey) -> Option<Arc<SyncGroup>> {
        self.registry
            .lock()
            .membership
            .get(key)
            .map(|(group, _)| group.clone())
    }

    /// The group with this id, if any.
    pub fn group_by_id(&self, group_id: u64) -> Option<Arc<SyncGroup>> {
        self.registry
            .lock()
            .groups
            .iter()
            .find(|row| row.group.id() == group_id)
            .map(|row| row.group.clone())
    }

    /// The display name of the group with this id, if any.
    pub fn group_name(&self, group_id: u64) -> Option<String> {
        self.registry
            .lock()
            .groups
            .iter()
            .find(|row| row.group.id() == group_id)
            .map(|row| row.name.clone())
    }

    /// Register the member session a group's `start` created. The entry
    /// claims the receiver (see [`SyncController::uses_receiver`]) until it
    /// is taken back.
    pub fn set_playing(&self, key: &OutputKey, session: OutputSession) {
        self.registry.lock().playing.insert(key.clone(), session);
    }

    /// Take back one member's claim, returning its session so the caller
    /// can stop it.
    pub fn take_playing(&self, key: &OutputKey) -> Option<OutputSession> {
        self.registry.lock().playing.remove(key)
    }

    /// The session currently playing this output as a sync-group member.
    pub fn playing_session(&self, key: &OutputKey) -> Option<OutputSession> {
        self.registry.lock().playing.get(key).cloned()
    }

    /// Whether a sync-group member session currently drives this receiver.
    /// Two sessions claiming one receiver is a hard refusal.
    pub fn uses_receiver(&self, transport: &str, device_id: &str) -> bool {
        self.playing_session(&OutputKey::new(transport, device_id))
            .is_some()
    }

    /// Explicit re-entry into sync after one output's transport restarted
    /// mid-flight (roadmap B9: the Sonos stall recovery must not resume
    /// playback at a wall-clock instant of its own choosing). Returns the
    /// member's [`StartPlan`]: when the group's own presentation epoch is
    /// still pending that epoch, so the restart joins the coordinated start
    /// instead of racing it; otherwise the plan for `at`, whose delay and
    /// latency are the same presentation contract the member's feed
    /// (delay-line pre-fill) already applies. The stale drift report is
    /// dropped first, so [`SyncController::sync_status`] reports
    /// [`SyncState::Syncing`] from this moment until the drift servo
    /// re-locks — the restart is visible as re-synchronizing, never as
    /// still-locked. `None` means the output is free-running: no epoch to
    /// re-enter, so the restart may fire immediately.
    pub fn plan_resync(
        &self,
        output: &on_air_api::ActiveOutput,
        at: std::time::Instant,
    ) -> Option<StartPlan> {
        let key = OutputKey::new(&output.transport, &output.device_id);
        let group = {
            let mut registry = self.registry.lock();
            let group = registry.membership.get(&key)?.0.clone();
            if let Some(row) = registry.outputs.get_mut(&key) {
                row.report = None;
            }
            group
        };
        let epoch = group
            .epoch()
            .map(|epoch| epoch.start_at)
            .filter(|start| *start > at)
            .unwrap_or(at);
        group.start_plan(&key, epoch)
    }

    /// The user's latency trim for one output, in milliseconds (0 default).
    pub fn trim_ms(&self, key: &OutputKey) -> i32 {
        self.registry
            .lock()
            .outputs
            .get(key)
            .map(|row| row.trim_ms)
            .unwrap_or_default()
    }

    /// Set one output's latency trim. The change applies to that output's
    /// feed the next time it opens (the next activation or stream); a live
    /// delay line is not resized underneath a playing speaker.
    pub fn set_trim_ms(&self, key: &OutputKey, transport: &str, trim_ms: i32) -> LatencyEstimate {
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key.clone()).or_default();
        row.trim_ms = trim_ms;
        let measured = row.measured.as_ref().map(|m| m.latency_us);
        let estimate = latency::effective_latency(transport, measured, trim_ms);
        if let Some((group, member)) = registry.membership.get(key) {
            group.set_latency(*member, estimate);
        }
        estimate
    }

    /// Apply one output's acoustic measurement (roadmap B10). It replaces
    /// the documented default from here on (`calibrated` becomes true) and
    /// re-times a live sync-group delay line if the output is playing in
    /// one.
    pub fn set_calibration(&self, measurement: &CalibrationMeasurement) -> LatencyEstimate {
        let key = OutputKey::new(&measurement.transport, &measurement.device_id);
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key.clone()).or_default();
        row.measured = Some(measurement.clone());
        let estimate = latency::effective_latency(
            &measurement.transport,
            Some(measurement.latency_us),
            row.trim_ms,
        );
        if let Some((group, member)) = registry.membership.get(&key) {
            group.set_latency(*member, estimate);
        }
        estimate
    }

    /// The applied measurement for one output, if any.
    pub fn calibration(&self, key: &OutputKey) -> Option<CalibrationMeasurement> {
        self.registry
            .lock()
            .outputs
            .get(key)
            .and_then(|row| row.measured.clone())
    }

    /// Remember an output's display name for the sync status.
    pub fn note_name(&self, key: &OutputKey, name: &str) {
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key.clone()).or_default();
        if !name.is_empty() {
            row.name = name.to_string();
        }
    }

    /// The last display name seen for one output, if any.
    pub fn display_name(&self, key: &OutputKey) -> Option<String> {
        self.registry
            .lock()
            .outputs
            .get(key)
            .map(|row| row.name.clone())
            .filter(|name| !name.is_empty())
    }

    /// The effective latency (measurement or default, plus trim) for one
    /// output.
    pub fn effective_latency(&self, transport: &str, key: &OutputKey) -> LatencyEstimate {
        let registry = self.registry.lock();
        let row = registry.outputs.get(key);
        let (trim_ms, measured) = (
            row.map(|row| row.trim_ms).unwrap_or_default(),
            row.and_then(|row| row.measured.as_ref().map(|m| m.latency_us)),
        );
        latency::effective_latency(transport, measured, trim_ms)
    }

    /// The feed handle for one output, if it is a sync-group member.
    /// `rate_hz` is the rate the feed serves at (its delay line's frames).
    pub fn feed_for(
        self: &Arc<Self>,
        output: &on_air_api::ActiveOutput,
        rate_hz: u32,
    ) -> Option<SyncFeed> {
        let key = OutputKey::new(&output.transport, &output.device_id);
        let group = self.group_of(&key)?;
        let delay = group.delay_frames(&key, rate_hz)?;
        {
            let mut registry = self.registry.lock();
            let row = registry.outputs.entry(key.clone()).or_default();
            row.name = output.device_name.clone();
            row.rate_hz = Some(rate_hz);
        }
        Some(SyncFeed {
            output: output.clone(),
            delay_frames: delay,
            rate_hz,
            controller: Arc::clone(self),
        })
    }

    /// Record one consumer's drift measurement for the API's sync status.
    pub fn report_drift(&self, output: &on_air_api::ActiveOutput, report: Option<DriftReport>) {
        let key = OutputKey::new(&output.transport, &output.device_id);
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key).or_default();
        row.name = output.device_name.clone();
        row.report = report;
    }

    /// Whether one output is synchronized (in a group) right now.
    pub fn is_synchronized(&self, output: &on_air_api::ActiveOutput) -> bool {
        self.group_of(&OutputKey::new(&output.transport, &output.device_id))
            .is_some()
    }

    /// Every named group with its members and playing state, for the wire.
    pub fn group_views(&self) -> Vec<SyncGroupView> {
        let registry = self.registry.lock();
        registry
            .groups
            .iter()
            .map(|row| {
                let members = row
                    .group
                    .members()
                    .into_iter()
                    .map(|(_member, key, _latency)| {
                        let name = registry
                            .outputs
                            .get(&key)
                            .map(|row| row.name.clone())
                            .unwrap_or_default();
                        let snapshot = registry.playing.get(&key).map(|s| s.snapshot());
                        let playing = matches!(
                            snapshot.as_ref().map(|s| s.phase),
                            Some(OutputPhase::Starting | OutputPhase::Live)
                        );
                        let error = snapshot.and_then(|s| s.error);
                        SyncGroupMember {
                            transport: key.transport.clone(),
                            device_id: key.device_id.clone(),
                            device_name: name,
                            playing,
                            error,
                        }
                    })
                    .collect::<Vec<_>>();
                let playing = members.iter().any(|member| member.playing);
                SyncGroupView {
                    id: row.group.id(),
                    name: row.name.clone(),
                    members,
                    playing,
                }
            })
            .collect()
    }

    /// Groups and members for `settings.json`, in creation order.
    pub fn saved_groups(&self) -> Vec<SavedSyncGroup> {
        let registry = self.registry.lock();
        registry
            .groups
            .iter()
            .map(|row| SavedSyncGroup {
                name: row.name.clone(),
                members: row
                    .group
                    .members()
                    .into_iter()
                    .map(|(_member, key, _latency)| {
                        let device_name = registry
                            .outputs
                            .get(&key)
                            .map(|row| row.name.clone())
                            .unwrap_or_default();
                        SavedSyncGroupMember {
                            transport: key.transport,
                            device_id: key.device_id,
                            device_name,
                        }
                    })
                    .collect(),
            })
            .collect()
    }

    /// Put one persisted group back: created and stopped, exactly as the
    /// user left it in configuration (playback never survives a restart).
    pub fn restore_group(self: &Arc<Self>, saved: &SavedSyncGroup) {
        let group = self.create_group(saved.name.clone());
        for member in &saved.members {
            let _ = self.join(
                &group,
                &on_air_api::ActiveOutput {
                    transport: member.transport.clone(),
                    device_id: member.device_id.clone(),
                    device_name: member.device_name.clone(),
                },
            );
        }
    }

    /// Per-output sync status: one row per output this core knows about,
    /// merged with `live` (the currently active output(s) and their rates).
    pub fn sync_status(&self, live: &[(on_air_api::ActiveOutput, u32)]) -> Vec<OutputSyncStatus> {
        let registry = self.registry.lock();
        let mut keys: Vec<OutputKey> = registry.outputs.keys().cloned().collect();
        for (output, _) in live {
            let key = OutputKey::new(&output.transport, &output.device_id);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys.sort_by(|a, b| (&a.transport, &a.device_id).cmp(&(&b.transport, &b.device_id)));
        keys.iter()
            .map(|key| {
                let row = registry.outputs.get(key);
                let live_rate = live
                    .iter()
                    .find(|(output, _)| {
                        output.transport == key.transport && output.device_id == key.device_id
                    })
                    .map(|(_, rate)| *rate);
                let name = row
                    .map(|row| row.name.clone())
                    .or_else(|| {
                        live.iter()
                            .find(|(output, _)| {
                                output.transport == key.transport
                                    && output.device_id == key.device_id
                            })
                            .map(|(output, _)| output.device_name.clone())
                    })
                    .unwrap_or_default();
                let trim_ms = row.map(|row| row.trim_ms).unwrap_or_default();
                let measured = row.and_then(|row| row.measured.as_ref().map(|m| m.latency_us));
                let default_latency_ms = latency::default_latency_ms(&key.transport);
                let effective_latency_ms =
                    latency::effective_latency_ms(&key.transport, measured, trim_ms);
                let calibration = row.and_then(|row| row.measured.clone());
                let rate_hz = live_rate
                    .or_else(|| row.and_then(|row| row.rate_hz))
                    .unwrap_or(FALLBACK_RATE_HZ);
                let synchronized = registry.membership.contains_key(key);
                let report = row.and_then(|row| row.report);
                let (state, drift_ppm) = if !synchronized {
                    (SyncState::FreeRunning, None)
                } else {
                    match report {
                        Some(report) => (report.state, Some(report.drift_ppm)),
                        None => (SyncState::Syncing, None),
                    }
                };
                let delay_frames = registry
                    .membership
                    .get(key)
                    .and_then(|(group, _)| group.delay_frames(key, rate_hz))
                    .unwrap_or_default();
                let group_id = registry
                    .membership
                    .get(key)
                    .map(|(group, _)| group.id());
                OutputSyncStatus {
                    transport: key.transport.clone(),
                    device_id: key.device_id.clone(),
                    device_name: name,
                    state,
                    trim_ms,
                    default_latency_ms,
                    effective_latency_ms,
                    calibrated: calibration.is_some(),
                    calibration,
                    delay_frames,
                    output_sample_rate_hz: rate_hz,
                    drift_ppm,
                    group_id,
                }
            })
            .collect()
    }
}

impl Default for SyncController {
    fn default() -> Self {
        Self::new()
    }
}

/// One output's sync feed: the handle its stream body or OS-sink pump holds
/// while that output is a sync-group member. Cheap to clone.
#[derive(Clone)]
pub struct SyncFeed {
    output: on_air_api::ActiveOutput,
    delay_frames: u64,
    rate_hz: u32,
    controller: Arc<SyncController>,
}

impl std::fmt::Debug for SyncFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncFeed")
            .field("output", &self.output)
            .field("delay_frames", &self.delay_frames)
            .field("rate_hz", &self.rate_hz)
            .finish_non_exhaustive()
    }
}

impl SyncFeed {
    /// The delay line length this feed must hold, in frames at [`Self::rate_hz`].
    pub fn delay_frames(&self) -> u64 {
        self.delay_frames
    }

    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    pub fn output(&self) -> &on_air_api::ActiveOutput {
        &self.output
    }

    /// Publish this consumer's drift measurement to the sync status.
    pub fn report(&self, drift: &DriftController) {
        self.controller.report_drift(&self.output, drift.report());
    }
}
