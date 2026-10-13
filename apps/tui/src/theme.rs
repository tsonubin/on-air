//! The one color borrowed from Omarchy: `accent` from the current theme's
//! `colors.toml`. Everything else comes from the terminal palette. A theme
//! switch swaps the whole `current/theme` directory, so `refresh` watches
//! the directory's mtime and re-reads the file when it changes. Nothing is
//! ever written under `~/.config/omarchy`.

use ratatui::style::Color;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const FALLBACK_ACCENT: Color = Color::Cyan;
const COLORS_FILE: &str = "colors.toml";

/// Where the current theme lives: Omarchy 4 first, then Omarchy 3.
pub fn candidate_dirs(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".local/state/omarchy/current/theme"),
        home.join(".config/omarchy/current/theme"),
    ]
}

/// The `accent = "#rrggbb"` line, wherever it is in the file. Hand parsed:
/// the file is small and this is the only key read.
pub fn parse_accent(colors_toml: &str) -> Option<Color> {
    for line in colors_toml.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("accent") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let value = rest.trim().split('#').nth(1)?;
        let hex: String = value
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() != 6 {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
    }
    None
}

pub struct Theme {
    dir: Option<PathBuf>,
    mtime: Option<SystemTime>,
    pub accent: Color,
}

fn mtime_of(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir).ok()?.modified().ok()
}

fn read_accent(dir: &Path) -> Option<Color> {
    let text = std::fs::read_to_string(dir.join(COLORS_FILE)).ok()?;
    parse_accent(&text)
}

impl Theme {
    pub fn load(home: &Path) -> Self {
        let dir = candidate_dirs(home)
            .into_iter()
            .find(|dir| dir.join(COLORS_FILE).is_file());
        let mtime = dir.as_deref().and_then(mtime_of);
        let accent = dir
            .as_deref()
            .and_then(read_accent)
            .unwrap_or(FALLBACK_ACCENT);
        Theme { dir, mtime, accent }
    }

    /// Re-read the accent when the theme directory changed. Cheap: one
    /// `stat` per call until something changes.
    pub fn refresh(&mut self) -> bool {
        let Some(dir) = &self.dir else {
            return false;
        };
        let mtime = mtime_of(dir);
        if mtime == self.mtime {
            return false;
        }
        self.mtime = mtime;
        let accent = read_accent(dir).unwrap_or(FALLBACK_ACCENT);
        if accent == self.accent {
            return false;
        }
        self.accent = accent;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("on-air-theme-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_accent_reads_the_hex_line_in_any_section() {
        assert_eq!(
            parse_accent("[colors]\naccent = \"#ffd60a\"\n"),
            Some(Color::Rgb(255, 214, 10))
        );
        assert_eq!(
            parse_accent("mode = \"dark\"\n\n[colors]\nforeground = \"#ffffff\"\naccent = '#00AAff' # note\n"),
            Some(Color::Rgb(0, 170, 255))
        );
        assert_eq!(parse_accent("foreground = \"#ffffff\"\n"), None);
        assert_eq!(parse_accent("accent = \"#fff\"\n"), None);
        assert_eq!(parse_accent("accent = \"#zzzzzz\"\n"), None);
    }

    #[test]
    fn candidate_dirs_prefer_omarchy_4_then_3() {
        let dirs = candidate_dirs(Path::new("/home/x"));
        assert_eq!(
            dirs[0],
            PathBuf::from("/home/x/.local/state/omarchy/current/theme")
        );
        assert_eq!(
            dirs[1],
            PathBuf::from("/home/x/.config/omarchy/current/theme")
        );
    }

    #[test]
    fn without_omarchy_the_accent_is_the_fallback() {
        let home = temp_home("none");
        let theme = Theme::load(&home);
        assert_eq!(theme.accent, FALLBACK_ACCENT);
    }

    #[test]
    fn refresh_notices_a_theme_switch_and_ignores_the_same_content() {
        let home = temp_home("switch");
        let dir = candidate_dirs(&home)[0].clone();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), "accent = \"#ffd60a\"\n").unwrap();
        let mut theme = Theme::load(&home);
        assert_eq!(theme.accent, Color::Rgb(255, 214, 10));
        assert!(!theme.refresh(), "same content is not a change");
        // A theme switch replaces the directory: new dir, new mtime, new file.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::remove_dir_all(&dir).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), "accent = \"#00ff00\"\n").unwrap();
        assert!(theme.refresh());
        assert_eq!(theme.accent, Color::Rgb(0, 255, 0));
    }
}
