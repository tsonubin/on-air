//! Sync groups: outputs that play together.
//!
//! The outputs of a sync group share one [`PresentationEpoch`] — a scheduled
//! wall-clock start instead of fire-now — and hold per-output delay lines so
//! a fast transport waits for the slowest member. A sync group is *more*
//! constrained than two unrelated outputs, not less: two outputs outside any
//! group free-run and drift apart, which is fine and expected, while two
//! outputs inside a group are held to the shared epoch and the delay lines.
//!
//! Scheduling is pre-rolled, not guessed: each member's feed presents its
//! delay of silence before the timeline (the delay line's pre-fill), so
//! issuing `start()` at the epoch begins every member mid-pre-roll and the
//! timeline sounds aligned at `epoch.start_at + baseline_latency`.

use crate::sync::latency::LatencyEstimate;
use std::time::{Duration, Instant};

/// Identifies one output across sessions: transport plus device.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OutputKey {
    pub transport: String,
    pub device_id: String,
}

impl OutputKey {
    pub fn new(transport: impl Into<String>, device_id: impl Into<String>) -> Self {
        OutputKey {
            transport: transport.into(),
            device_id: device_id.into(),
        }
    }
}

/// A sync group member handle, valid until [`SyncGroup::leave`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemberId(u64);

/// The common start instant every member of a sync group works toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationEpoch {
    /// Monotonic instant at which every member must issue its transport
    /// start (`Play`, opening the OS sink, ...). Acoustic alignment then
    /// comes from each member's latency plus its delay line: every member
    /// presents timeline frame 0 at `start_at + baseline_latency`.
    pub start_at: Instant,
}

/// How one member must begin playing against its group's epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartPlan {
    pub epoch: PresentationEpoch,
    /// How long this member's feed must hold the timeline back: the group
    /// baseline latency minus this member's own latency. Applied as a delay
    /// line of [`StartPlan::delay_frames`] frames at the feed's rate.
    pub delay: Duration,
    /// This member's latency (documented default plus user trim).
    pub effective_latency: Duration,
    /// The slowest member's latency; every member presents the timeline at
    /// `epoch.start_at + baseline_latency`.
    pub baseline_latency: Duration,
}

impl StartPlan {
    /// The feed delay in frames at `rate_hz`.
    pub fn delay_frames(&self, rate_hz: u32) -> u64 {
        delay_frames(self.delay, rate_hz)
    }

    /// Wait for the shared start instant instead of firing immediately.
    /// For a start already in the past this returns at once.
    pub async fn wait(&self) {
        let now = Instant::now();
        if let Some(delay) = self.epoch.start_at.checked_duration_since(now) {
            tokio::time::sleep(delay).await;
        }
    }
}

/// Frames of delay for a hold of `delay` at `rate_hz`.
pub fn delay_frames(delay: Duration, rate_hz: u32) -> u64 {
    (delay.as_secs_f64() * f64::from(rate_hz)).round() as u64
}

/// One member's plan against a presentation epoch.
fn plan_for(member: &Member, baseline: Duration, epoch: PresentationEpoch) -> StartPlan {
    StartPlan {
        epoch,
        delay: baseline.saturating_sub(member.latency.latency),
        effective_latency: member.latency.latency,
        baseline_latency: baseline,
    }
}

struct Member {
    id: MemberId,
    key: OutputKey,
    latency: LatencyEstimate,
}

#[derive(Default)]
struct GroupState {
    next_id: u64,
    members: Vec<Member>,
    epoch: Option<PresentationEpoch>,
}

/// Outputs that must play together.
pub struct SyncGroup {
    id: u64,
    state: parking_lot::Mutex<GroupState>,
}

impl SyncGroup {
    pub(crate) fn new(id: u64) -> Self {
        SyncGroup {
            id,
            state: parking_lot::Mutex::new(GroupState::default()),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Add an output to the group with its effective latency.
    pub fn join(&self, key: OutputKey, latency: LatencyEstimate) -> MemberId {
        let mut state = self.state.lock();
        let id = MemberId(state.next_id);
        state.next_id += 1;
        state.members.push(Member { id, key, latency });
        id
    }

    /// Remove a member. With the last member gone the epoch is forgotten.
    pub fn leave(&self, id: MemberId) {
        let mut state = self.state.lock();
        state.members.retain(|member| member.id != id);
        if state.members.is_empty() {
            state.epoch = None;
        }
    }

    /// Update a member's latency, e.g. after the user moves its trim.
    pub fn set_latency(&self, id: MemberId, latency: LatencyEstimate) {
        let mut state = self.state.lock();
        if let Some(member) = state.members.iter_mut().find(|member| member.id == id) {
            member.latency = latency;
        }
    }

    /// The member handle for one output, if it is in this group.
    pub fn member_of(&self, key: &OutputKey) -> Option<MemberId> {
        self.state
            .lock()
            .members
            .iter()
            .find(|member| member.key == *key)
            .map(|member| member.id)
    }

    /// (member, key, latency) for every member.
    pub fn members(&self) -> Vec<(MemberId, OutputKey, LatencyEstimate)> {
        self.state
            .lock()
            .members
            .iter()
            .map(|member| (member.id, member.key.clone(), member.latency))
            .collect()
    }

    /// The slowest member's latency: the group's presentation baseline.
    pub fn baseline_latency(&self) -> Duration {
        self.state
            .lock()
            .members
            .iter()
            .map(|member| member.latency.latency)
            .max()
            .unwrap_or_default()
    }

    /// The delay line one member's feed must hold, in frames at
    /// `sample_rate_hz`: the baseline minus that member's own latency, so
    /// fast transports wait for slow ones.
    pub fn delay_frames(&self, key: &OutputKey, sample_rate_hz: u32) -> Option<u64> {
        let state = self.state.lock();
        let member = state.members.iter().find(|member| member.key == *key)?;
        let baseline = state
            .members
            .iter()
            .map(|member| member.latency.latency)
            .max()
            .unwrap_or_default();
        Some(delay_frames(
            baseline.saturating_sub(member.latency.latency),
            sample_rate_hz,
        ))
    }

    /// Pre-roll and schedule: every member gets its [`StartPlan`] for the
    /// common epoch `at`, and the group remembers the epoch. All plans carry
    /// the same `epoch`; members differ only in their delay lines.
    pub fn schedule_start(&self, at: Instant) -> Vec<(MemberId, StartPlan)> {
        let mut state = self.state.lock();
        let epoch = PresentationEpoch { start_at: at };
        state.epoch = Some(epoch);
        let baseline = state
            .members
            .iter()
            .map(|member| member.latency.latency)
            .max()
            .unwrap_or_default();
        state
            .members
            .iter()
            .map(|member| (member.id, plan_for(member, baseline, epoch)))
            .collect()
    }

    /// One member's [`StartPlan`] for epoch `at`, without touching the
    /// group's remembered epoch. This is the plan a mid-flight re-entry —
    /// a transport that restarted and must begin playing again — starts
    /// against. `None` when `key` is not a member of this group.
    pub fn start_plan(&self, key: &OutputKey, at: Instant) -> Option<StartPlan> {
        let state = self.state.lock();
        let member = state.members.iter().find(|member| member.key == *key)?;
        let baseline = state
            .members
            .iter()
            .map(|member| member.latency.latency)
            .max()
            .unwrap_or_default();
        Some(plan_for(
            member,
            baseline,
            PresentationEpoch { start_at: at },
        ))
    }

    /// The epoch the last [`SyncGroup::schedule_start`] set, if any.
    pub fn epoch(&self) -> Option<PresentationEpoch> {
        self.state.lock().epoch
    }
}
