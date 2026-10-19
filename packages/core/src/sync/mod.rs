//! Cross-protocol playout synchronization.
//!
//! This is the foundation for mixed protocols playing in sync (roadmap item
//! 2). Three pieces cooperate:
//!
//! - a [`SyncGroup`] (see [`group`]) gives several outputs one presentation
//!   epoch and per-output delay lines, so fast transports wait for slow
//!   ones;
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

pub use drift::{DriftController, DriftReport};
pub use group::{delay_frames, MemberId, OutputKey, PresentationEpoch, StartPlan, SyncGroup};
pub use latency::{default_latency_ms, effective_latency, effective_latency_ms, LatencyEstimate};

use on_air_api::{OutputSyncStatus, SyncState};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Rate assumed for outputs whose feed rate was never recorded, only used to
/// size a delay line in a status row.
const FALLBACK_RATE_HZ: u32 = 48_000;

/// Per-output state the registry remembers: name, trim, the rate its feed
/// runs at, and the last drift report its feed made.
#[derive(Default)]
struct OutputRow {
    name: String,
    trim_ms: i32,
    rate_hz: Option<u32>,
    report: Option<DriftReport>,
}

#[derive(Default)]
struct Registry {
    groups: Vec<Arc<SyncGroup>>,
    next_group_id: u64,
    membership: HashMap<OutputKey, (Arc<SyncGroup>, MemberId)>,
    outputs: HashMap<OutputKey, OutputRow>,
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

    /// A fresh sync group. Outputs join it via [`SyncController::join`].
    pub fn create_group(self: &Arc<Self>) -> Arc<SyncGroup> {
        let mut registry = self.registry.lock();
        let group = Arc::new(SyncGroup::new(registry.next_group_id));
        registry.next_group_id += 1;
        registry.groups.push(group.clone());
        group
    }

    /// Join `output` to `group` at its effective latency (documented default
    /// plus the user's trim). The previous group, if any, is left behind.
    pub fn join(
        self: &Arc<Self>,
        group: &Arc<SyncGroup>,
        output: &on_air_api::ActiveOutput,
    ) -> MemberId {
        let key = OutputKey::new(&output.transport, &output.device_id);
        let latency = {
            let mut registry = self.registry.lock();
            let row = registry.outputs.entry(key.clone()).or_default();
            row.name = output.device_name.clone();
            let trim_ms = row.trim_ms;
            latency::effective_latency(&output.transport, trim_ms)
        };
        // An output plays in at most one group at a time.
        let _ = self.leave(&key);
        let member = group.join(key.clone(), latency);
        self.registry
            .lock()
            .membership
            .insert(key, (group.clone(), member));
        member
    }

    /// Remove one output from its group, if any.
    pub fn leave(&self, key: &OutputKey) -> Option<(Arc<SyncGroup>, MemberId)> {
        let mut registry = self.registry.lock();
        let entry = registry.membership.remove(key)?;
        entry.0.leave(entry.1);
        Some(entry)
    }

    /// The group one output belongs to, if any.
    pub fn group_of(&self, key: &OutputKey) -> Option<Arc<SyncGroup>> {
        self.registry
            .lock()
            .membership
            .get(key)
            .map(|(group, _)| group.clone())
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
        let estimate = latency::effective_latency(transport, trim_ms);
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key.clone()).or_default();
        row.trim_ms = trim_ms;
        if let Some((group, member)) = registry.membership.get(key) {
            group.set_latency(*member, estimate);
        }
        estimate
    }

    /// Remember an output's display name for the sync status.
    pub fn note_name(&self, key: &OutputKey, name: &str) {
        let mut registry = self.registry.lock();
        let row = registry.outputs.entry(key.clone()).or_default();
        if !name.is_empty() {
            row.name = name.to_string();
        }
    }

    /// The effective latency (default plus trim) for one output.
    pub fn effective_latency(&self, transport: &str, key: &OutputKey) -> LatencyEstimate {
        latency::effective_latency(transport, self.trim_ms(key))
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
                let default_latency_ms = latency::default_latency_ms(&key.transport);
                let effective_latency_ms = latency::effective_latency_ms(&key.transport, trim_ms);
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
                OutputSyncStatus {
                    transport: key.transport.clone(),
                    device_id: key.device_id.clone(),
                    device_name: name,
                    state,
                    trim_ms,
                    default_latency_ms,
                    effective_latency_ms,
                    calibrated: false,
                    delay_frames,
                    output_sample_rate_hz: rate_hz,
                    drift_ppm,
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
