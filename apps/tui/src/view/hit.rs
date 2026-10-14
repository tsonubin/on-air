//! Screen regions the mouse can act on, recorded while rendering.

use crate::model::MixerRow;
use ratatui::layout::{Position, Rect};

/// The CD transport buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdButton {
    Prev,
    PlayPause,
    Next,
    Eject,
}

/// What a clicked region stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    SourceRow(usize),
    DestRow(usize),
    Fader(MixerRow),
    Rate(MixerRow),
    CdButton(CdButton),
    CdProgress,
    MonitorTag,
    StatusTag,
    Overlay,
    OverlayOutside,
}

/// Regions in paint order; the last match wins so overlays beat what is under them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HitMap(Vec<(Rect, Target)>);

impl HitMap {
    pub fn push(&mut self, rect: Rect, target: Target) {
        self.0.push((rect, target));
    }

    pub fn hit(&self, column: u16, row: u16) -> Option<(Target, Rect)> {
        let position = Position::new(column, row);
        self.0
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(position))
            .map(|(rect, target)| (*target, *rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_prefers_the_last_pushed_rect() {
        let mut map = HitMap::default();
        map.push(Rect::new(0, 0, 10, 10), Target::SourceRow(0));
        map.push(Rect::new(2, 2, 4, 4), Target::Overlay);
        assert_eq!(
            map.hit(3, 3),
            Some((Target::Overlay, Rect::new(2, 2, 4, 4)))
        );
        assert_eq!(
            map.hit(8, 8),
            Some((Target::SourceRow(0), Rect::new(0, 0, 10, 10)))
        );
        assert_eq!(map.hit(20, 20), None);
    }
}
