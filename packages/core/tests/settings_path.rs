use on_air_core::settings_path_from;
use std::path::PathBuf;

fn p(s: &str) -> Option<PathBuf> {
    Some(PathBuf::from(s))
}

#[test]
fn an_explicit_config_dir_wins() {
    assert_eq!(
        settings_path_from(p("/srv/on-air"), p("/home/u/.cfg"), p("/home/u")),
        p("/srv/on-air/settings.json")
    );
}

#[test]
fn xdg_config_home_comes_before_home() {
    assert_eq!(
        settings_path_from(None, p("/home/u/.cfg"), p("/home/u")),
        p("/home/u/.cfg/on-air-core/settings.json")
    );
}

#[test]
fn home_falls_back_to_dot_config() {
    assert_eq!(
        settings_path_from(None, None, p("/home/u")),
        p("/home/u/.config/on-air-core/settings.json")
    );
}

#[test]
fn relative_or_missing_locations_are_ignored() {
    assert_eq!(
        settings_path_from(p("relative"), p(""), p("/home/u")),
        p("/home/u/.config/on-air-core/settings.json")
    );
    assert_eq!(settings_path_from(None, None, None), None);
    assert_eq!(settings_path_from(None, None, p("")), None);
}
