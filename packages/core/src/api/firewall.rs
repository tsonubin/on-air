//! Administrator-authorized firewall repair, available only on the host.
use crate::api::{auth::LocalClient, error::ApiError};
use crate::firewall::valid_subnet;
use crate::state::CoreState;
use axum::{extract::State, http::StatusCode};
use std::{sync::atomic::Ordering, time::Duration};
const REPAIR_SCRIPT: &str = include_str!("../firewall_allow.sh");

/// No parameters are accepted: the subnet and port come from observed local
/// traffic. Remote clients get instructions, never a host privilege prompt.
pub async fn repair(
    LocalClient: LocalClient,
    State(state): State<CoreState>,
) -> Result<StatusCode, ApiError> {
    let _repair = state
        .firewall_repair_lock
        .try_lock()
        .map_err(|_| ApiError::conflict("a firewall repair is already awaiting authorization"))?;
    let warning = state
        .firewall_warning
        .lock()
        .clone()
        .ok_or_else(|| ApiError::not_ready("no observed UFW block needs repair"))?;
    if !valid_subnet(&warning.subnet) || warning.port != state.serve_port.load(Ordering::Acquire) {
        return Err(ApiError::validation("invalid firewall repair scope"));
    }
    if !state.mock {
        if !crate::platform::FIREWALL_REPAIR {
            return Err(ApiError::not_ready("UFW repair is only available on Linux"));
        }
        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("pkexec")
                .args(["--disable-internal-agent", "/bin/sh", "-c", REPAIR_SCRIPT, "on-air-firewall", &warning.subnet, &warning.port.to_string()])
                .kill_on_drop(true)
                .output(),
        ).await
            .map_err(|_| ApiError::not_ready("administrator authorization timed out; retry or run the displayed command on the host"))?
            .map_err(|_| ApiError::not_ready("could not open administrator authorization; run the displayed command on the host"))?;
        if !output.status.success() {
            return Err(ApiError::not_ready("firewall rule was not applied; authorization was cancelled or UFW could not update its rules. Run the displayed command on the host."));
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs().saturating_add(1));
    state.firewall_repaired_at.store(now, Ordering::Release);
    *state.firewall_warning.lock() = None;
    Ok(StatusCode::NO_CONTENT)
}
