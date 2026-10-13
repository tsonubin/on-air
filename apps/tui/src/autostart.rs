//! Start the service at login. On Linux that is the `on-air-core` systemd
//! user unit the package installs; elsewhere the desktop app owns it.

pub const UNIT: &str = "on-air-core";

pub fn systemctl_args(enabled: bool) -> Vec<&'static str> {
    if enabled {
        vec!["--user", "enable", "--now", UNIT]
    } else {
        vec!["--user", "disable", UNIT]
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{systemctl_args, UNIT};
    use tokio::process::Command;

    fn failed(what: &str, error: impl std::fmt::Display) -> String {
        format!("could not {what} the {UNIT} user service: {error}")
    }

    /// `systemctl --user is-enabled on-air-core`: exit 0 means enabled; any
    /// other exit (disabled, not found, no systemd) means not enabled.
    pub async fn is_enabled() -> Result<bool, String> {
        let output = Command::new("systemctl")
            .args(["--user", "is-enabled", UNIT])
            .output()
            .await
            .map_err(|error| failed("query", error))?;
        Ok(output.status.success())
    }

    pub async fn set_enabled(enabled: bool) -> Result<bool, String> {
        let output = Command::new("systemctl")
            .args(systemctl_args(enabled))
            .output()
            .await
            .map_err(|error| failed("change", error))?;
        if output.status.success() {
            Ok(enabled)
        } else {
            Err(failed(
                "change",
                String::from_utf8_lossy(&output.stderr).trim(),
            ))
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    const NOT_HERE: &str = "autostart is managed by the desktop app on this platform";

    pub async fn is_enabled() -> Result<bool, String> {
        Err(NOT_HERE.to_string())
    }

    pub async fn set_enabled(_enabled: bool) -> Result<bool, String> {
        Err(NOT_HERE.to_string())
    }
}

pub use imp::{is_enabled, set_enabled};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemctl_enables_now_and_disables() {
        assert_eq!(
            systemctl_args(true),
            vec!["--user", "enable", "--now", "on-air-core"]
        );
        assert_eq!(
            systemctl_args(false),
            vec!["--user", "disable", "on-air-core"]
        );
        assert_eq!(UNIT, "on-air-core");
    }
}
