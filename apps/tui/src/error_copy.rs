//! The one place that turns a failure into words. Branches on the core's
//! error `code`, then on the HTTP status, never on message text, with the
//! same sentences as `apps/desktop/src/lib/errorCopy.ts`.

use crate::client::ClientError;

pub const UNREACHABLE_PREFIX: &str = "Core unreachable on ";
pub const TIMEOUT_COPY: &str = "The audio service did not answer in time.";
/// Shown when the core reports that the active source stopped on its own.
pub const INPUT_STOPPED_COPY: &str = "The source stopped unexpectedly. Choose it again to restart.";
/// The notice line when the service is on but no sleep lock is held.
pub const SLEEP_NOT_INHIBITED_COPY: &str =
    "sleep not inhibited — the laptop may suspend while streaming (see docs/omarchy.md)";

fn copy_by_code(code: &str) -> Option<&'static str> {
    Some(match code {
        "service_paused" => "The audio service is paused. Turn it on to continue.",
        "not_paired" => "This window is not paired with the audio service. Restart on-air.",
        "invalid_pin" => "That PIN did not match. Check the code on the speaker and try again.",
        "pin_lockout" => "Too many PIN attempts. Wait a minute, then try again.",
        "transport_unreachable" => {
            "The speaker did not answer. Check it is on and on this network."
        }
        "conflict" => "Another output is still switching. Try again in a moment.",
        "not_ready" => "That speaker isn't ready. Check it's on and connected, then try again.",
        "no_active_output" => "Nothing is playing yet. Pick a speaker first.",
        "not_found" => "That device is no longer available. Refresh devices and try again.",
        "invalid_request" => "The audio service rejected that request.",
        "forbidden" => "The audio service refused that request.",
        "method_not_allowed" => "The audio service does not support that action.",
        "internal" => "The audio service hit an internal error.",
        _ => return None,
    })
}

fn copy_by_status(status: u16) -> Option<&'static str> {
    let code = match status {
        401 => "not_paired",
        404 => "not_found",
        409 => "conflict",
        429 => "pin_lockout",
        502 => "transport_unreachable",
        503 => "service_paused",
        _ => return None,
    };
    copy_by_code(code)
}

pub fn copy(error: &ClientError) -> String {
    match error {
        ClientError::Unreachable(base) => format!(
            "{UNREACHABLE_PREFIX}{base}; is on-air-core running? (systemctl --user status on-air-core)"
        ),
        ClientError::Timeout => TIMEOUT_COPY.to_string(),
        ClientError::Http { status, code, .. } => code
            .as_deref()
            .and_then(copy_by_code)
            .or_else(|| copy_by_status(*status))
            .map(str::to_string)
            .unwrap_or_else(|| format!("The audio service answered {status}.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16, code: Option<&str>) -> ClientError {
        ClientError::Http {
            status,
            code: code.map(str::to_string),
            message: String::new(),
        }
    }

    #[test]
    fn the_code_wins_over_the_status() {
        assert_eq!(
            copy(&http(409, Some("not_ready"))),
            "That speaker isn't ready. Check it's on and connected, then try again."
        );
        assert_eq!(
            copy(&http(409, None)),
            "Another output is still switching. Try again in a moment."
        );
    }

    #[test]
    fn unknown_codes_and_statuses_fall_back_to_the_status() {
        assert_eq!(
            copy(&http(418, Some("teapot"))),
            "The audio service answered 418."
        );
    }

    #[test]
    fn timeouts_and_unreachable_cores_have_their_own_copy() {
        assert_eq!(copy(&ClientError::Timeout), TIMEOUT_COPY);
        assert_eq!(
            copy(&ClientError::Unreachable("http://x:1".into())),
            "Core unreachable on http://x:1; is on-air-core running? (systemctl --user status on-air-core)"
        );
    }
}
