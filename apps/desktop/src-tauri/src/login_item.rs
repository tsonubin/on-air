//! macOS login autostart.
//!
//! A LaunchAgent that runs the bare executable is started by launchd as a
//! daemon. The webview in that process never attaches to the window server, so
//! opening the window from the tray shows an empty UI. Launch `/usr/bin/open`
//! inside an Aqua session instead; `open` hands the app bundle to LaunchServices.
//!
//! Other platforms keep their login item in the OS through
//! `tauri-plugin-autostart`; [`autostart_plan`] decides when to touch it. The
//! saved preference (`autostart.json`) is the source of truth everywhere, and
//! with no saved preference nothing is installed: autostart is opt-in.

// The LaunchAgent half is only called on macOS; the tests cover it everywhere.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};

pub const LAUNCH_AGENT_LABEL: &str = "on-air-desktop";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlistKind {
    Absent,
    /// Runs a bare executable directly. launchd treats that as a daemon.
    LegacyBareExe,
    OpensBundle(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginItemAction {
    Install { bundle: PathBuf },
    Remove,
    Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupPlan {
    pub checked: bool,
    pub action: LoginItemAction,
    /// Write this preference when it was only implied by an older install.
    pub persist: Option<bool>,
}

pub fn launched_hidden<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--hidden")
}

/// `…/Name.app/Contents/MacOS/binary` → `…/Name.app`.
pub fn bundle_from_exe(exe: &Path) -> Option<PathBuf> {
    let exe = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let mut parts: Vec<_> = exe.components().collect();
    if parts.len() < 4 {
        return None;
    }
    parts.pop()?;
    let macos = parts.pop()?;
    let contents = parts.pop()?;
    let app = parts.last()?;
    if macos.as_os_str() != "MacOS" || contents.as_os_str() != "Contents" {
        return None;
    }
    if !app.as_os_str().to_string_lossy().ends_with(".app") {
        return None;
    }
    Some(parts.iter().collect())
}

pub fn launch_agent_plist(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

pub fn preference_path(config_dir: &Path) -> PathBuf {
    config_dir.join("autostart.json")
}

pub fn load_preference(config_dir: &Path) -> Option<bool> {
    let bytes = std::fs::read(preference_path(config_dir)).ok()?;
    let value: Preference = serde_json::from_slice(&bytes).ok()?;
    Some(value.enabled)
}

pub fn save_preference(config_dir: &Path, enabled: bool) -> std::io::Result<()> {
    if let Some(parent) = preference_path(config_dir).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(&Preference { enabled })
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(preference_path(config_dir), bytes)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Preference {
    enabled: bool,
}

pub fn read_plist_kind(path: &Path) -> PlistKind {
    match std::fs::read_to_string(path) {
        Ok(xml) => classify_plist(&xml),
        Err(_) => PlistKind::Absent,
    }
}

pub fn classify_plist(xml: &str) -> PlistKind {
    let args = program_arguments(xml);
    if args.first().map(String::as_str) == Some("/usr/bin/open") {
        if let Some(flag) = args.iter().position(|arg| arg == "-a") {
            if let Some(bundle) = args.get(flag + 1) {
                if bundle.ends_with(".app") {
                    return PlistKind::OpensBundle(PathBuf::from(bundle));
                }
            }
        }
    }
    if args.is_empty() && !xml.contains("<key>ProgramArguments</key>") {
        return PlistKind::Absent;
    }
    PlistKind::LegacyBareExe
}

pub fn startup_plan(
    preference: Option<bool>,
    plist: &PlistKind,
    bundle: Option<&Path>,
) -> StartupPlan {
    match preference {
        Some(false) => StartupPlan {
            checked: false,
            action: LoginItemAction::Remove,
            persist: None,
        },
        Some(true) => match bundle {
            Some(bundle) => StartupPlan {
                checked: true,
                action: LoginItemAction::Install {
                    bundle: bundle.to_path_buf(),
                },
                persist: None,
            },
            None => match plist {
                PlistKind::OpensBundle(_) => StartupPlan {
                    checked: true,
                    action: LoginItemAction::Leave,
                    persist: None,
                },
                _ => StartupPlan {
                    checked: false,
                    action: LoginItemAction::Remove,
                    persist: Some(false),
                },
            },
        },
        // No saved choice. An agent left by an older install that opens a
        // bundle counts as that choice and is recorded; nothing else installs
        // one, so a first launch never adds a login item.
        None => match (plist, bundle) {
            (PlistKind::OpensBundle(installed), Some(bundle)) if installed.as_path() == bundle => {
                StartupPlan {
                    checked: true,
                    action: LoginItemAction::Leave,
                    persist: Some(true),
                }
            }
            // The app moved; keep the user's choice pointed at where it is now.
            (PlistKind::OpensBundle(_), Some(bundle)) => StartupPlan {
                checked: true,
                action: LoginItemAction::Install {
                    bundle: bundle.to_path_buf(),
                },
                persist: Some(true),
            },
            (PlistKind::OpensBundle(_), None) => StartupPlan {
                checked: true,
                action: LoginItemAction::Leave,
                persist: Some(true),
            },
            (PlistKind::LegacyBareExe, _) => StartupPlan {
                checked: false,
                action: LoginItemAction::Remove,
                persist: None,
            },
            (PlistKind::Absent, _) => StartupPlan {
                checked: false,
                action: LoginItemAction::Leave,
                persist: None,
            },
        },
    }
}

/// What to do with an OS-managed login item (`tauri-plugin-autostart`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub struct AutostartPlan {
    pub checked: bool,
    /// Enable (`Some(true)`) or disable (`Some(false)`) the OS login item;
    /// `None` leaves it alone.
    pub apply: Option<bool>,
    /// Write this preference when it was only implied by the OS state.
    pub persist: Option<bool>,
}

/// The OS login item is touched only when the saved preference disagrees
/// with it. With no saved preference the OS state is reported as is: a first
/// launch installs nothing, and an entry left by an older build that
/// enabled autostart by default is kept and recorded as the user's choice.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn autostart_plan(preference: Option<bool>, os_enabled: bool) -> AutostartPlan {
    match preference {
        Some(wanted) => AutostartPlan {
            checked: wanted,
            apply: (wanted != os_enabled).then_some(wanted),
            persist: None,
        },
        None => AutostartPlan {
            checked: os_enabled,
            apply: None,
            persist: os_enabled.then_some(true),
        },
    }
}

pub fn user_choice(enabled: bool, bundle: Option<&Path>) -> Result<LoginItemAction, String> {
    if enabled {
        let Some(bundle) = bundle else {
            return Err(
                "Open at login needs the installed on-air app. This development build loads its window from the dev server, so it cannot start at login."
                    .into(),
            );
        };
        Ok(LoginItemAction::Install {
            bundle: bundle.to_path_buf(),
        })
    } else {
        Ok(LoginItemAction::Remove)
    }
}

pub fn plist_for_bundle(bundle: &Path) -> String {
    let bundle = xml_escape(&bundle.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCH_AGENT_LABEL}</string>
  <key>LimitLoadToSessionType</key>
  <string>Aqua</string>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>-g</string>
    <string>-a</string>
    <string>{bundle}</string>
    <string>--args</string>
    <string>--hidden</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    )
}

pub fn apply_action(home: &Path, action: &LoginItemAction) -> std::io::Result<()> {
    let path = launch_agent_plist(home);
    match action {
        LoginItemAction::Leave => Ok(()),
        LoginItemAction::Remove => {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            Ok(())
        }
        LoginItemAction::Install { bundle } => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, plist_for_bundle(bundle))
        }
    }
}

pub fn remove_legacy_agent(home: &Path) {
    let path = launch_agent_plist(home);
    if matches!(read_plist_kind(&path), PlistKind::LegacyBareExe) {
        let _ = std::fs::remove_file(path);
    }
}

fn program_arguments(xml: &str) -> Vec<String> {
    let Some(key) = xml.find("<key>ProgramArguments</key>") else {
        return Vec::new();
    };
    let Some(array_at) = xml[key..].find("<array>") else {
        return Vec::new();
    };
    let from = key + array_at;
    let Some(end_at) = xml[from..].find("</array>") else {
        return Vec::new();
    };
    let array = &xml[from..from + end_at];
    let mut rest = array;
    let mut args = Vec::new();
    while let Some(open) = rest.find("<string>") {
        rest = &rest[open + "<string>".len()..];
        let Some(close) = rest.find("</string>") else {
            break;
        };
        args.push(xml_unescape(&rest[..close]));
        rest = &rest[close + "</string>".len()..];
    }
    args
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_flag_is_the_login_argument() {
        assert!(launched_hidden(["on-air", "--hidden"]));
        assert!(!launched_hidden(["on-air", "--minimized"]));
    }

    #[test]
    fn bundle_path_requires_the_app_layout() {
        let exe = Path::new("/Applications/on-air-desktop.app/Contents/MacOS/on-air-desktop");
        assert_eq!(
            bundle_from_exe(exe),
            Some(PathBuf::from("/Applications/on-air-desktop.app"))
        );
        assert_eq!(
            bundle_from_exe(Path::new("/tmp/target/debug/on-air-desktop")),
            None
        );
    }

    #[test]
    fn generated_plist_opens_the_bundle_in_an_aqua_session() {
        let xml = plist_for_bundle(Path::new("/Apps/on-air & co.app"));
        assert!(xml.contains("<string>Aqua</string>"));
        assert!(xml.contains("<string>Interactive</string>"));
        assert!(xml.contains("<string>/usr/bin/open</string>"));
        assert!(xml.contains("<string>-g</string>"));
        assert!(xml.contains("<string>/Apps/on-air &amp; co.app</string>"));
        assert!(xml.contains("<string>--hidden</string>"));
        assert!(xml.contains("<true/>"));
        assert!(matches!(
            classify_plist(&xml),
            PlistKind::OpensBundle(path) if path == Path::new("/Apps/on-air & co.app")
        ));
    }

    #[test]
    fn legacy_debug_agent_is_not_treated_as_a_bundle_launch() {
        let xml = r#"<?xml version="1.0"?>
<plist><dict>
  <key>ProgramArguments</key>
  <array><string>/Users/shay/Projects/oss/on-air/target/debug/on-air-desktop</string><string>--hidden</string></array>
</dict></plist>"#;
        assert_eq!(classify_plist(xml), PlistKind::LegacyBareExe);
    }

    #[test]
    fn startup_removes_a_bare_executable_and_keeps_a_working_bundle() {
        let bundle = Path::new("/Applications/on-air-desktop.app");
        let legacy = startup_plan(None, &PlistKind::LegacyBareExe, None);
        assert!(!legacy.checked);
        assert_eq!(legacy.action, LoginItemAction::Remove);

        let installed = startup_plan(None, &PlistKind::OpensBundle(bundle.into()), None);
        assert!(installed.checked);
        assert_eq!(installed.action, LoginItemAction::Leave);

        let off = startup_plan(
            Some(false),
            &PlistKind::OpensBundle(bundle.into()),
            Some(bundle),
        );
        assert!(!off.checked);
        assert_eq!(off.action, LoginItemAction::Remove);
    }

    #[test]
    fn first_launch_of_a_bundle_installs_nothing() {
        let bundle = Path::new("/Applications/on-air-desktop.app");
        let fresh = startup_plan(None, &PlistKind::Absent, Some(bundle));
        assert_eq!(
            fresh,
            StartupPlan {
                checked: false,
                action: LoginItemAction::Leave,
                persist: None,
            }
        );

        let legacy = startup_plan(None, &PlistKind::LegacyBareExe, Some(bundle));
        assert!(!legacy.checked);
        assert_eq!(legacy.action, LoginItemAction::Remove);
        assert_eq!(legacy.persist, None);

        let opted_in = startup_plan(Some(true), &PlistKind::Absent, Some(bundle));
        assert!(opted_in.checked);
        assert_eq!(
            opted_in.action,
            LoginItemAction::Install {
                bundle: bundle.into()
            }
        );
    }

    #[test]
    fn an_older_install_keeps_its_login_item_and_records_the_choice() {
        let bundle = Path::new("/Applications/on-air-desktop.app");
        let same = startup_plan(None, &PlistKind::OpensBundle(bundle.into()), Some(bundle));
        assert!(same.checked);
        assert_eq!(same.action, LoginItemAction::Leave);
        assert_eq!(same.persist, Some(true));

        let moved = startup_plan(
            None,
            &PlistKind::OpensBundle(PathBuf::from("/Users/me/Downloads/on-air-desktop.app")),
            Some(bundle),
        );
        assert!(moved.checked);
        assert_eq!(
            moved.action,
            LoginItemAction::Install {
                bundle: bundle.into()
            }
        );
        assert_eq!(moved.persist, Some(true));

        let dev = startup_plan(None, &PlistKind::OpensBundle(bundle.into()), None);
        assert!(dev.checked);
        assert_eq!(dev.action, LoginItemAction::Leave);
        assert_eq!(dev.persist, Some(true));
    }

    #[test]
    fn os_login_item_is_touched_only_when_the_preference_disagrees() {
        assert_eq!(
            autostart_plan(None, false),
            AutostartPlan {
                checked: false,
                apply: None,
                persist: None,
            },
            "first launch: off and nothing installed"
        );
        assert_eq!(
            autostart_plan(None, true),
            AutostartPlan {
                checked: true,
                apply: None,
                persist: Some(true),
            },
            "an entry from an older build is kept and recorded"
        );
        assert_eq!(
            autostart_plan(Some(true), true),
            AutostartPlan {
                checked: true,
                apply: None,
                persist: None,
            },
            "already in step: no enable() call on this launch"
        );
        assert_eq!(autostart_plan(Some(true), false).apply, Some(true));
        assert_eq!(autostart_plan(Some(false), true).apply, Some(false));
        assert_eq!(autostart_plan(Some(false), false).apply, None);
        assert!(!autostart_plan(Some(false), true).checked);
    }

    #[test]
    fn development_build_cannot_opt_into_login_launch() {
        let error = user_choice(false, None).unwrap();
        assert_eq!(error, LoginItemAction::Remove);
        assert!(user_choice(true, None).is_err());
        assert!(matches!(
            user_choice(true, Some(Path::new("/Applications/on-air-desktop.app"))).unwrap(),
            LoginItemAction::Install { .. }
        ));
    }

    #[test]
    fn preference_round_trip_and_agent_removal() {
        let dir = std::env::temp_dir().join(format!(
            "on-air-login-item-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let home = dir.join("home");
        std::fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
        let config = dir.join("config");
        save_preference(&config, false).unwrap();
        assert_eq!(load_preference(&config), Some(false));

        let legacy = launch_agent_plist(&home);
        std::fs::write(
            &legacy,
            r#"<plist><dict><key>ProgramArguments</key><array><string>/tmp/on-air-desktop</string></array></dict></plist>"#,
        )
        .unwrap();
        remove_legacy_agent(&home);
        assert!(!legacy.exists());

        apply_action(
            &home,
            &LoginItemAction::Install {
                bundle: PathBuf::from("/Applications/on-air-desktop.app"),
            },
        )
        .unwrap();
        assert!(matches!(
            read_plist_kind(&launch_agent_plist(&home)),
            PlistKind::OpensBundle(_)
        ));
        apply_action(&home, &LoginItemAction::Remove).unwrap();
        assert!(matches!(
            read_plist_kind(&launch_agent_plist(&home)),
            PlistKind::Absent
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    }
}
