//! Terminal mouse events to messages, through the hit map of the last draw.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use on_air_tui::update::{MouseKind, Msg};
use on_air_tui::view::{HitMap, Target};
use ratatui::layout::Rect;

/// What the left button went down on, so a drag keeps acting on that fader
/// wherever the pointer wanders.
#[derive(Debug, Default)]
pub(crate) struct DragState {
    origin: Option<(Target, Rect)>,
}

impl DragState {
    /// The message for a mouse event, or `None` for a cell no target covers
    /// (including cells past the last frame after a resize), for a drag that
    /// did not start on a fader, and for kinds the UI does not use.
    pub(crate) fn mouse_msg(&mut self, map: &HitMap, event: MouseEvent) -> Option<Msg> {
        let (column, row) = (event.column, event.row);
        let (kind, (target, rect), row) = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.origin = map.hit(column, row);
                (MouseKind::Down, self.origin?, row)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let origin @ (Target::Fader(_), rect) = self.origin? else {
                    return None;
                };
                let row = row.clamp(rect.y, rect.bottom().saturating_sub(1));
                (MouseKind::Drag, origin, row)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.origin = None;
                return None;
            }
            MouseEventKind::ScrollUp => (MouseKind::ScrollUp, map.hit(column, row)?, row),
            MouseEventKind::ScrollDown => (MouseKind::ScrollDown, map.hit(column, row)?, row),
            _ => return None,
        };
        Some(Msg::Mouse {
            target,
            rect,
            column,
            row,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use on_air_tui::model::{MixerRow, Model};
    use on_air_tui::update::update;

    const VOLUME: Rect = Rect::new(10, 5, 3, 5);
    const BAND: Rect = Rect::new(14, 5, 3, 5);

    fn event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn down(column: u16, row: u16) -> MouseEvent {
        event(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    fn drag(column: u16, row: u16) -> MouseEvent {
        event(MouseEventKind::Drag(MouseButton::Left), column, row)
    }

    fn map() -> HitMap {
        let mut map = HitMap::default();
        map.push(VOLUME, Target::Fader(MixerRow::Volume));
        map.push(BAND, Target::Fader(MixerRow::Eq(0)));
        map.push(Rect::new(0, 12, 20, 1), Target::SourceRow(0));
        map
    }

    fn msg(target: Target, rect: Rect, column: u16, row: u16, kind: MouseKind) -> Msg {
        Msg::Mouse {
            target,
            rect,
            column,
            row,
            kind,
        }
    }

    #[test]
    fn mouse_kinds_map_to_messages() {
        let map = map();
        let volume = Target::Fader(MixerRow::Volume);
        let mut state = DragState::default();
        assert_eq!(
            state.mouse_msg(&map, down(11, 7)),
            Some(msg(volume, VOLUME, 11, 7, MouseKind::Down))
        );
        assert_eq!(
            state.mouse_msg(&map, drag(11, 8)),
            Some(msg(volume, VOLUME, 11, 8, MouseKind::Drag))
        );
        for (kind, want) in [
            (MouseEventKind::ScrollUp, MouseKind::ScrollUp),
            (MouseEventKind::ScrollDown, MouseKind::ScrollDown),
        ] {
            assert_eq!(
                state.mouse_msg(&map, event(kind, 15, 7)),
                Some(msg(Target::Fader(MixerRow::Eq(0)), BAND, 15, 7, want))
            );
        }
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollLeft,
        ] {
            assert_eq!(state.mouse_msg(&map, event(kind, 11, 7)), None, "{kind:?}");
        }
    }

    #[test]
    fn mouse_outside_every_target_is_ignored() {
        let map = map();
        let mut state = DragState::default();
        assert_eq!(state.mouse_msg(&map, down(0, 0)), None);
        assert_eq!(
            state.mouse_msg(&map, down(500, 300)),
            None,
            "beyond the map after a resize"
        );
        assert_eq!(state.mouse_msg(&HitMap::default(), down(11, 7)), None);
    }

    #[test]
    fn a_drag_stays_on_the_fader_it_started_on() {
        let map = map();
        let mut state = DragState::default();
        let mut model = Model::new();
        model.snapshot.volume = 50;
        let down = state.mouse_msg(&map, down(11, 7)).unwrap();
        assert_eq!(update(&mut model, down), vec![], "the middle cell is 50");
        let over_band = state.mouse_msg(&map, drag(15, 6)).unwrap();
        assert_eq!(
            over_band,
            msg(
                Target::Fader(MixerRow::Volume),
                VOLUME,
                15,
                6,
                MouseKind::Drag
            )
        );
        update(&mut model, over_band);
        assert_eq!(model.snapshot.volume, 75);
        assert_eq!(model.snapshot.eq, [0.0; 5], "the band is untouched");
    }

    #[test]
    fn a_drag_past_the_top_reaches_max() {
        let map = map();
        let mut state = DragState::default();
        let mut model = Model::new();
        update(&mut model, state.mouse_msg(&map, down(11, 8)).unwrap());
        let above = state.mouse_msg(&map, drag(11, 0)).unwrap();
        assert_eq!(
            above,
            msg(
                Target::Fader(MixerRow::Volume),
                VOLUME,
                11,
                VOLUME.y,
                MouseKind::Drag
            )
        );
        update(&mut model, above);
        assert_eq!(model.snapshot.volume, 100);
        let below = state.mouse_msg(&map, drag(11, 29)).unwrap();
        assert!(matches!(below, Msg::Mouse { row: 9, .. }), "{below:?}");
    }

    #[test]
    fn a_drag_needs_a_down_on_a_fader() {
        let map = map();
        let mut state = DragState::default();
        assert_eq!(state.mouse_msg(&map, drag(11, 7)), None, "no Down yet");
        state.mouse_msg(&map, down(1, 12));
        assert_eq!(state.mouse_msg(&map, drag(11, 7)), None, "started on a row");
        state.mouse_msg(&map, down(0, 0));
        assert_eq!(
            state.mouse_msg(&map, drag(11, 7)),
            None,
            "started on nothing"
        );
    }

    #[test]
    fn up_clears_the_origin() {
        let map = map();
        let mut state = DragState::default();
        state.mouse_msg(&map, down(11, 7));
        let up = event(MouseEventKind::Up(MouseButton::Left), 11, 7);
        assert_eq!(state.mouse_msg(&map, up), None);
        assert_eq!(state.mouse_msg(&map, drag(11, 7)), None);
    }
}
