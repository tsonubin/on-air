//! `on-air service start|stop|restart`: the `on-air-core` systemd user unit
//! itself, as opposed to `on-air monitor`, which switches audio on and off
//! inside a running core.

use crate::autostart::UNIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceVerb {
    Start,
    Stop,
    Restart,
}

impl ServiceVerb {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "restart" => Some(Self::Restart),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    #[cfg(target_os = "linux")]
    fn done(self) -> &'static str {
        match self {
            Self::Start => "started",
            Self::Stop => "stopped",
            Self::Restart => "restarted",
        }
    }
}

pub fn systemctl_args(verb: ServiceVerb) -> Vec<&'static str> {
    vec!["--user", verb.word(), UNIT]
}

/// systemctl's stderr when the unit file is not installed.
#[cfg(any(target_os = "linux", test))]
fn not_installed(stderr: &str) -> bool {
    stderr.contains("not found") || stderr.contains("not loaded")
}

#[cfg(target_os = "linux")]
pub async fn control(verb: ServiceVerb) -> Result<String, String> {
    let output = tokio::process::Command::new("systemctl")
        .args(systemctl_args(verb))
        .output()
        .await
        .map_err(|error| format!("could not run systemctl: {error}"))?;
    if output.status.success() {
        return Ok(format!("{UNIT} {}", verb.done()));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if not_installed(&stderr) {
        return Err(format!(
            "{UNIT} is not installed as a service; run packaging/omarchy/install.sh"
        ));
    }
    Err(format!(
        "could not {} the {UNIT} user service: {}",
        verb.word(),
        stderr.trim()
    ))
}

#[cfg(not(target_os = "linux"))]
pub async fn control(_verb: ServiceVerb) -> Result<String, String> {
    Err("the service is managed by the desktop app on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbs_parse_and_map_to_systemctl() {
        for (word, verb) in [
            ("start", ServiceVerb::Start),
            ("stop", ServiceVerb::Stop),
            ("restart", ServiceVerb::Restart),
        ] {
            assert_eq!(ServiceVerb::parse(word), Some(verb));
            assert_eq!(systemctl_args(verb), vec!["--user", word, "on-air-core"]);
        }
        assert_eq!(ServiceVerb::parse("on"), None);
        assert_eq!(ServiceVerb::parse("enable"), None);
    }

    #[test]
    fn a_missing_unit_is_recognised() {
        assert!(not_installed(
            "Failed to start on-air-core.service: Unit on-air-core.service not found."
        ));
        assert!(!not_installed("Access denied"));
    }
}
