//! The user-facing sync-group lifecycle behind `/api/sync/groups`.
//!
//! The resource model, stated once (the API and every client share it):
//!
//! - Sync groups are NAMED and MANY. A group is configuration: creating,
//!   reshaping or dissolving one never disturbs what is playing right now.
//! - An output is a member of AT MOST ONE group at a time. Putting an
//!   output that already plays in another group into this one is REFUSED;
//!   take it out of the other group first.
//! - Membership takes effect at the group's next [`start`]: an output added
//!   while it plays free-running keeps free-running until then.
//! - [`start`] is the one action that makes sound. It schedules ONE
//!   presentation epoch for the whole group and activates every member on
//!   its own [`crate::session::OutputSession`] against that epoch (roadmap
//!   B6). The members then play together — but they remain SEVERAL
//!   SPEAKERS, never one merged device: each keeps its own transport,
//!   volume, latency default and trim.
//! - Starting a group TAKES OVER a member that is playing free-running (its
//!   free-running session stops and it begins with the group). A receiver
//!   driven by any other session — a patchbay route, another on-air
//!   session — is refused: two sessions claiming one receiver is a hard
//!   refusal (`session::activate`).
//! - Leaving a group ([`remove_member`]) stops that member cleanly if it
//!   was playing with the group; the other members keep playing. Stopping
//!   or dissolving a group likewise stops only its own members; a
//!   free-running output is never touched.

use super::{JoinError, OutputKey, StartPlan};
use crate::events::WsEvent;
use crate::state::CoreState;
use on_air_api::ActiveOutput;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Longest accepted group display name, in characters.
pub const NAME_MAX_CHARS: usize = 64;

/// How far in the future a group's shared presentation epoch is scheduled.
/// Stopping a previous sender happens before the epoch is scheduled; the
/// pre-roll only separates preparation from "issue the transport start", so
/// every member can gather on one instant ([`StartPlan::wait`]).
pub const START_PRE_ROLL: Duration = Duration::from_millis(250);

/// Why a sync-group lifecycle call was refused. The API layer maps these to
/// the one error envelope; clients branch on the `code`, never on text.
#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error("sync group not found")]
    NotFound,
    #[error("output device not found")]
    OutputNotFound,
    #[error("a group name must be 1 to {NAME_MAX_CHARS} characters")]
    InvalidName,
    /// The output is a member of another group already.
    #[error("this output already plays in group `{group}`; take it out first")]
    AlreadyInGroup { group: String },
    #[error("this output does not play in that sync group")]
    NotAMember,
    #[error("a sync group needs at least one member to start")]
    Empty,
    #[error("the on-air service is paused")]
    Paused,
    /// Another session drives one of the receivers.
    #[error("{0}")]
    Busy(String),
    /// A member could not be stopped, so nothing was changed.
    #[error("could not stop {member}: {error}")]
    StopFailed { member: String, error: String },
    /// Starting is per member: some members play, these did not.
    #[error("{}", .0.join("; "))]
    StartFailed(Vec<String>),
}

/// Create an empty named sync group. Nothing plays until [`start`].
pub async fn create(state: &CoreState, name: &str) -> Result<u64, GroupError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
        return Err(GroupError::InvalidName);
    }
    let _configuration = state.config_lock.lock().await;
    let group = state.sync.create_group(name);
    let id = group.id();
    state.remember_sync_groups();
    emit_changed(state, id);
    Ok(id)
}

/// Put one output (a real device from `GET /api/outputs`) into a group.
/// Re-adding a member of this same group is a no-op; an output that plays
/// in ANOTHER group is refused.
pub async fn add_member(
    state: &CoreState,
    group_id: u64,
    transport: &str,
    device_id: &str,
) -> Result<(), GroupError> {
    let device_name = crate::session::list(state)
        .await
        .into_iter()
        .find(|info| info.transport == transport && info.id == device_id)
        .map(|info| info.name)
        .ok_or(GroupError::OutputNotFound)?;
    let _configuration = state.config_lock.lock().await;
    let group = state
        .sync
        .group_by_id(group_id)
        .ok_or(GroupError::NotFound)?;
    let output = ActiveOutput {
        transport: transport.to_string(),
        device_id: device_id.to_string(),
        device_name,
    };
    state
        .sync
        .join(&group, &output)
        .map_err(|error| match error {
            JoinError::AlreadyInGroup => GroupError::AlreadyInGroup {
                group: current_group_name(state, &output),
            },
        })?;
    state.remember_sync_groups();
    emit_changed(state, group_id);
    Ok(())
}

/// Take one output out of a group. If it is playing with the group it is
/// stopped cleanly first and the other members keep playing; on a failed
/// stop nothing changes.
pub async fn remove_member(
    state: &CoreState,
    group_id: u64,
    transport: &str,
    device_id: &str,
) -> Result<(), GroupError> {
    let _configuration = state.config_lock.lock().await;
    state
        .sync
        .group_by_id(group_id)
        .ok_or(GroupError::NotFound)?;
    let key = OutputKey::new(transport, device_id);
    if state.sync.group_of(&key).map(|group| group.id()) != Some(group_id) {
        return Err(GroupError::NotAMember);
    }
    stop_member(state, &key).await?;
    state.sync.leave(&key);
    state.remember_sync_groups();
    emit_changed(state, group_id);
    Ok(())
}

/// Dissolve a group: stop every playing member cleanly, then forget the
/// group and its membership.
pub async fn dissolve(state: &CoreState, group_id: u64) -> Result<(), GroupError> {
    let _configuration = state.config_lock.lock().await;
    let group = state
        .sync
        .group_by_id(group_id)
        .ok_or(GroupError::NotFound)?;
    let keys: Vec<OutputKey> = group
        .members()
        .into_iter()
        .map(|(_member, key, _latency)| key)
        .collect();
    for key in &keys {
        stop_member(state, key).await?;
    }
    state.sync.dissolve_group(group_id);
    state.remember_sync_groups();
    emit_changed(state, group_id);
    Ok(())
}

/// Stop every member of a group cleanly. Membership stays; the group is
/// simply no longer playing.
pub async fn stop(state: &CoreState, group_id: u64) -> Result<(), GroupError> {
    let _configuration = state.config_lock.lock().await;
    let group = state
        .sync
        .group_by_id(group_id)
        .ok_or(GroupError::NotFound)?;
    let keys: Vec<OutputKey> = group
        .members()
        .into_iter()
        .map(|(_member, key, _latency)| key)
        .collect();
    for key in &keys {
        stop_member(state, key).await?;
    }
    emit_changed(state, group_id);
    Ok(())
}

/// Start every member of a group at ONE presentation epoch (roadmap B6).
///
/// Each member runs on its own session, concurrently, so no member waits
/// behind another's transport start and none fires now: every sender is
/// issued at `epoch.start_at`. A member playing free-running is taken over;
/// a receiver another session drives is refused up front and nothing
/// starts. Starting is per member afterwards: a member whose transport
/// fails does not stop the others (see [`GroupError::StartFailed`]).
pub async fn start(state: &CoreState, group_id: u64) -> Result<(), GroupError> {
    let _configuration = state.config_lock.lock().await;
    if !state
        .service_enabled
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(GroupError::Paused);
    }
    let group = state
        .sync
        .group_by_id(group_id)
        .ok_or(GroupError::NotFound)?;
    let members = group.members();
    if members.is_empty() {
        return Err(GroupError::Empty);
    }

    // Preflight: no receiver may be driven by another session. (A member
    // this group already plays is fine: its restart is a switch of its own
    // session at the new epoch, never a second claimant.)
    for (_member, key, _latency) in &members {
        if state
            .patchbay
            .uses_receiver(&key.transport, &key.device_id)
            .await
        {
            return Err(GroupError::Busy(format!(
                "{} is connected to a routed cable. Disconnect its route first.",
                display_name(state, key)
            )));
        }
    }

    // Take over a free-running root output that is one of the members.
    if let Some(active) = state.active_output() {
        let active_key = OutputKey::new(&active.transport, &active.device_id);
        if members.iter().any(|(_, key, _)| *key == active_key) {
            state
                .output()
                .deactivate()
                .await
                .map_err(|error| GroupError::StopFailed {
                    member: active.device_name,
                    error: error.to_string(),
                })?;
        }
    }

    // One shared presentation epoch for the whole group.
    let epoch = Instant::now() + START_PRE_ROLL;
    let plans: HashMap<super::MemberId, StartPlan> =
        group.schedule_start(epoch).into_iter().collect();

    let mut starts = tokio::task::JoinSet::new();
    for (member, key, _latency) in &members {
        // Reuse the member's session when it already plays with this group:
        // restarting is a switch at the new epoch, never a second session.
        let child = {
            let mut children = state.group_children.lock().await;
            match children.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let child = state.group_child();
                    entry.insert(child.clone());
                    child
                }
            }
        };
        state.sync.set_playing(key, child.output().clone());
        let plan = plans[member];
        let key = key.clone();
        starts.spawn(async move {
            let result =
                crate::session::activate_scheduled(&child, &key.transport, &key.device_id, plan)
                    .await;
            (key, child, result)
        });
    }

    let mut failures = Vec::new();
    while let Some(started) = starts.join_next().await {
        let (key, child, result) = started.expect("a member start does not panic");
        if let Err(error) = result {
            // A rolled-back member must not keep claiming its receiver.
            if child.output().snapshot().phase == on_air_api::OutputPhase::Idle {
                state.sync.take_playing(&key);
                state.group_children.lock().await.remove(&key);
            }
            failures.push(format!("{}: {error}", display_name(state, &key)));
        }
    }
    emit_changed(state, group_id);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(GroupError::StartFailed(failures))
    }
}

/// Stop one member's session and drop its claim. A failed stop keeps the
/// claim (the receiver may still be sounding) and changes nothing else.
async fn stop_member(state: &CoreState, key: &OutputKey) -> Result<(), GroupError> {
    let Some(session) = state.sync.playing_session(key) else {
        return Ok(());
    };
    if let Err(error) = session.deactivate().await {
        return Err(GroupError::StopFailed {
            member: display_name(state, key),
            error: error.to_string(),
        });
    }
    state.sync.take_playing(key);
    state.group_children.lock().await.remove(key);
    Ok(())
}

fn display_name(state: &CoreState, key: &OutputKey) -> String {
    state
        .sync
        .display_name(key)
        .unwrap_or_else(|| key.device_id.clone())
}

fn current_group_name(state: &CoreState, output: &ActiveOutput) -> String {
    let key = OutputKey::new(&output.transport, &output.device_id);
    state
        .sync
        .group_of(&key)
        .and_then(|group| state.sync.group_name(group.id()))
        .unwrap_or_else(|| "another group".to_string())
}

/// One `SyncGroupChanged` event: the clients re-read `GET /api/sync`.
fn emit_changed(state: &CoreState, group_id: u64) {
    let _ = state.ws_tx.send(WsEvent::SyncGroupChanged { group_id });
}
