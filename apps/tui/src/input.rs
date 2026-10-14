//! Terminal mouse events to messages, through the hit map of the last draw.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use on_air_tui::update::{MouseKind, Msg};
use on_air_tui::view::HitMap;

/// The message for a mouse event, or `None` for a cell no target covers
/// (including cells past the last frame after a resize) and for kinds the
/// UI does not use.
pub(crate) fn mouse_msg(map: &HitMap, event: MouseEvent) -> Option<Msg> {
    let kind = match event.kind {
        MouseEventKind::Down(MouseButton::Left) => MouseKind::Down,
        MouseEventKind::Drag(MouseButton::Left) => MouseKind::Drag,
        MouseEventKind::ScrollUp => MouseKind::ScrollUp,
        MouseEventKind::ScrollDown => MouseKind::ScrollDown,
        _ => return None,
    };
    let (target, rect) = map.hit(event.column, event.row)?;
    Some(Msg::Mouse {
        target,
        rect,
        column: event.column,
        row: event.row,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use on_air_tui::model::MixerRow;
    use on_air_tui::view::Target;
    use ratatui::layout::Rect;

    fn event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn map() -> HitMap {
        let mut map = HitMap::default();
        map.push(Rect::new(10, 5, 3, 5), Target::Fader(MixerRow::Volume));
        map
    }

    #[test]
    fn mouse_kinds_map_to_messages() {
        let map = map();
        let rect = Rect::new(10, 5, 3, 5);
        for (kind, want) in [
            (MouseEventKind::Down(MouseButton::Left), MouseKind::Down),
            (MouseEventKind::Drag(MouseButton::Left), MouseKind::Drag),
            (MouseEventKind::ScrollUp, MouseKind::ScrollUp),
            (MouseEventKind::ScrollDown, MouseKind::ScrollDown),
        ] {
            assert_eq!(
                mouse_msg(&map, event(kind, 11, 7)),
                Some(Msg::Mouse {
                    target: Target::Fader(MixerRow::Volume),
                    rect,
                    column: 11,
                    row: 7,
                    kind: want,
                })
            );
        }
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollLeft,
        ] {
            assert_eq!(mouse_msg(&map, event(kind, 11, 7)), None, "{kind:?}");
        }
    }

    #[test]
    fn mouse_outside_every_target_is_ignored() {
        let map = map();
        let down = MouseEventKind::Down(MouseButton::Left);
        assert_eq!(mouse_msg(&map, event(down, 0, 0)), None);
        assert_eq!(
            mouse_msg(&map, event(down, 500, 300)),
            None,
            "beyond the map after a resize"
        );
        assert_eq!(mouse_msg(&HitMap::default(), event(down, 11, 7)), None);
    }
}
