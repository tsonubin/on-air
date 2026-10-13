use on_air_core::settings_path_from;
use std::path::PathBuf;

/// An absolute directory on every platform (`/srv` is relative on Windows).
fn root() -> PathBuf {
    std::env::temp_dir().join("on-air-settings-path")
}

#[test]
fn an_explicit_config_dir_wins() {
    let r = root();
    assert_eq!(
        settings_path_from(
            Some(r.join("srv")),
            Some(r.join("cfg")),
            Some(r.join("home"))
        ),
        Some(r.join("srv").join("settings.json"))
    );
}

#[test]
fn xdg_config_home_comes_before_home() {
    let r = root();
    assert_eq!(
        settings_path_from(None, Some(r.join("cfg")), Some(r.join("home"))),
        Some(r.join("cfg").join("on-air-core").join("settings.json"))
    );
}

#[test]
fn home_falls_back_to_dot_config() {
    let r = root();
    assert_eq!(
        settings_path_from(None, None, Some(r.join("home"))),
        Some(
            r.join("home")
                .join(".config/on-air-core")
                .join("settings.json")
        )
    );
}

#[test]
fn relative_or_missing_locations_are_ignored() {
    let r = root();
    assert_eq!(
        settings_path_from(
            Some(PathBuf::from("relative")),
            Some(PathBuf::new()),
            Some(r.join("home"))
        ),
        Some(
            r.join("home")
                .join(".config/on-air-core")
                .join("settings.json")
        )
    );
    assert_eq!(settings_path_from(None, None, None), None);
    assert_eq!(settings_path_from(None, None, Some(PathBuf::new())), None);
}
