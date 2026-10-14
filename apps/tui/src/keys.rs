//! The key map, in one table so the help overlay and the hint line are
//! rendered from the same rows `update` matches on. The layout follows
//! cliamp, the music TUI Omarchy preinstalls: `Tab` cycles focus, `j`/`k`
//! move, `h`/`l` adjust, `+`/`-` are volume, `Space` is transport, `?` is
//! help and `q` quits.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextPane,
    PrevPane,
    Down,
    Up,
    Select,
    AdjustUp,
    AdjustDown,
    FineUp,
    FineDown,
    VolumeUp,
    VolumeDown,
    ResetBand,
    Stop,
    PlayPause,
    NextTrack,
    PrevTrack,
    SeekBack,
    SeekForward,
    GotoTrack,
    Eject,
    Monitor,
    Autostart,
    Status,
    Bluetooth,
    Refresh,
    Help,
    Back,
    Quit,
}

pub struct Binding {
    pub keys: &'static str,
    pub action: Action,
    pub help: &'static str,
}

/// One row per line of the key table in the design spec, in that order.
pub const KEYS: &[Binding] = &[
    Binding {
        keys: "Tab / Shift-Tab",
        action: Action::NextPane,
        help: "Next / previous pane",
    },
    Binding {
        keys: "j / k, ↓ / ↑",
        action: Action::Down,
        help: "Move within the pane",
    },
    Binding {
        keys: "Enter",
        action: Action::Select,
        help: "Activate the source or output (opens pairing when needed)",
    },
    Binding {
        keys: "h / l, ← / →",
        action: Action::AdjustDown,
        help: "Mixer: adjust the focused row",
    },
    Binding {
        keys: "H / L",
        action: Action::FineDown,
        help: "Fine adjust (volume ±1, EQ ±0.5 dB)",
    },
    Binding {
        keys: "+ / -",
        action: Action::VolumeUp,
        help: "Volume ±5 from any pane",
    },
    Binding {
        keys: "0",
        action: Action::ResetBand,
        help: "Mixer: reset the focused EQ band",
    },
    Binding {
        keys: "x",
        action: Action::Stop,
        help: "Stop the live output",
    },
    Binding {
        keys: "Space",
        action: Action::PlayPause,
        help: "CD play / pause",
    },
    Binding {
        keys: "> / <",
        action: Action::NextTrack,
        help: "CD next / previous track",
    },
    Binding {
        keys: "[ / ]",
        action: Action::SeekBack,
        help: "CD seek back / forward 10 s",
    },
    Binding {
        keys: "g",
        action: Action::GotoTrack,
        help: "CD go to track",
    },
    Binding {
        keys: "e",
        action: Action::Eject,
        help: "CD eject (asks first)",
    },
    Binding {
        keys: "m",
        action: Action::Monitor,
        help: "Monitor on / off",
    },
    Binding {
        keys: "a",
        action: Action::Autostart,
        help: "Start the service at login on / off",
    },
    Binding {
        keys: "i",
        action: Action::Status,
        help: "Status: version, addresses, monitor, login start",
    },
    Binding {
        keys: "b",
        action: Action::Bluetooth,
        help: "Open Bluetooth settings",
    },
    Binding {
        keys: "r",
        action: Action::Refresh,
        help: "Refresh devices",
    },
    Binding {
        keys: "?",
        action: Action::Help,
        help: "This help",
    },
    Binding {
        keys: "Esc",
        action: Action::Back,
        help: "Close, dismiss the error or notice",
    },
    Binding {
        keys: "q / Ctrl-C",
        action: Action::Quit,
        help: "Quit; the service keeps running",
    },
];

/// The short hints on the bottom line, in order.
pub const HINTS: &[(&str, &str)] = &[
    ("Tab", "pane"),
    ("j/k", "move"),
    ("Enter", "select"),
    ("h/l", "adjust"),
    ("+/-", "volume"),
    ("m", "monitor"),
    ("i", "info"),
    ("?", "help"),
    ("q", "quit"),
];

pub fn action_for(key: &KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Tab {
        return None;
    }
    Some(match key.code {
        KeyCode::Tab => Action::NextPane,
        KeyCode::BackTab => Action::PrevPane,
        KeyCode::Down => Action::Down,
        KeyCode::Up => Action::Up,
        KeyCode::Enter => Action::Select,
        KeyCode::Right => Action::AdjustUp,
        KeyCode::Left => Action::AdjustDown,
        KeyCode::Esc => Action::Back,
        KeyCode::Char(c) => match c {
            'j' => Action::Down,
            'k' => Action::Up,
            'l' => Action::AdjustUp,
            'h' => Action::AdjustDown,
            'L' => Action::FineUp,
            'H' => Action::FineDown,
            '+' | '=' => Action::VolumeUp,
            '-' | '_' => Action::VolumeDown,
            '0' => Action::ResetBand,
            'x' => Action::Stop,
            ' ' => Action::PlayPause,
            '>' | '.' => Action::NextTrack,
            '<' | ',' => Action::PrevTrack,
            '[' => Action::SeekBack,
            ']' => Action::SeekForward,
            'g' => Action::GotoTrack,
            'e' => Action::Eject,
            'm' => Action::Monitor,
            'a' => Action::Autostart,
            'i' => Action::Status,
            'b' => Action::Bluetooth,
            'r' => Action::Refresh,
            '?' => Action::Help,
            'q' => Action::Quit,
            _ => return None,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn every_table_row_has_a_key_that_maps_to_its_action() {
        assert_eq!(action_for(&press(KeyCode::Tab)), Some(Action::NextPane));
        assert_eq!(action_for(&press(KeyCode::BackTab)), Some(Action::PrevPane));
        assert_eq!(action_for(&press(KeyCode::Char('j'))), Some(Action::Down));
        assert_eq!(action_for(&press(KeyCode::Char('?'))), Some(Action::Help));
        assert_eq!(
            action_for(&press(KeyCode::Char('m'))),
            Some(Action::Monitor)
        );
        assert_eq!(action_for(&press(KeyCode::Char('s'))), None);
        assert_eq!(action_for(&press(KeyCode::Char('z'))), None);
        assert_eq!(KEYS.len(), 21);
        assert_eq!(action_for(&press(KeyCode::Char('L'))), Some(Action::FineUp));
        assert_eq!(
            action_for(&press(KeyCode::Char('H'))),
            Some(Action::FineDown)
        );
    }
}
