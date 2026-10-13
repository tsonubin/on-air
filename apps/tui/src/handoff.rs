//! Hand-offs to the desktop session. The core runs as a systemd user
//! service, which may have started before Hyprland exported its display
//! variables; the TUI runs inside the session. So when the TUI talks to a
//! local core it opens Bluetooth settings itself, and only asks the core
//! (`POST /api/bluetooth/settings`) for a remote one or when nothing here
//! could be started.

use std::process::{Command, Stdio};

/// Where to send the user to pair a speaker, first match wins; the same
/// order as the core's Linux list: Omarchy 4's shell panel, Omarchy 3's
/// `bluetui`, then GNOME, Blueman and KDE.
pub const SETTINGS_CANDIDATES: &[(&str, &[&str])] = &[
    ("omarchy-shell", &["shell", "toggle", "omarchy.bluetooth"]),
    ("omarchy-launch-tui", &["bluetui"]),
    ("gnome-control-center", &["bluetooth"]),
    ("blueman-manager", &[]),
    ("kcmshell6", &["kcm_bluetooth"]),
    ("kcmshell5", &["bluedevildevices"]),
];

/// Whether `base` points at this machine.
pub fn is_loopback_base(base: &str) -> bool {
    let rest = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))
        .unwrap_or(base);
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Start the first candidate that exists, detached from the TUI's terminal
/// (its output would garble the screen). True when one started.
pub fn spawn_first(candidates: &[(&str, &[&str])]) -> bool {
    for (program, args) in candidates {
        let spawned = Command::new(program)
            .args(*args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            // Reap it whenever it exits, off the UI thread.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return true;
        }
    }
    false
}

/// Open Bluetooth settings from this session. False when nothing here
/// could be started, so the caller can ask the core instead.
pub fn open_bluetooth_settings() -> bool {
    cfg!(target_os = "linux") && spawn_first(SETTINGS_CANDIDATES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_local_core_hands_off_to_this_session() {
        assert!(is_loopback_base("http://127.0.0.1:47990"));
        assert!(is_loopback_base("http://localhost:47990/"));
        assert!(is_loopback_base("http://[::1]:47990"));
        assert!(is_loopback_base("http://127.4.5.6"));
        assert!(!is_loopback_base("http://10.0.0.2:47990"));
        assert!(!is_loopback_base("http://localhost.example.com:47990"));
    }

    #[test]
    fn omarchy_panels_come_before_desktop_settings() {
        let programs: Vec<&str> = SETTINGS_CANDIDATES.iter().map(|(bin, _)| *bin).collect();
        assert_eq!(
            programs,
            vec![
                "omarchy-shell",
                "omarchy-launch-tui",
                "gnome-control-center",
                "blueman-manager",
                "kcmshell6",
                "kcmshell5"
            ]
        );
    }

    #[test]
    fn a_session_with_none_of_them_reports_failure() {
        assert!(!spawn_first(&[("on-air-no-such-program", &[])]));
        assert!(spawn_first(&[
            ("on-air-no-such-program", &[]),
            ("true", &[])
        ]));
    }
}
