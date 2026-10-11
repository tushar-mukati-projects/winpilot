use deckhand_core::Rect;
use windows::Win32::Foundation::RECT;

pub(crate) fn rect_from_win32(rect: RECT) -> Rect {
    Rect::new(rect.left, rect.top, rect.right.saturating_sub(rect.left), rect.bottom.saturating_sub(rect.top))
}

pub(crate) fn inflate(rect: Rect, by: i32) -> Rect {
    Rect::new(
        rect.x.saturating_sub(by),
        rect.y.saturating_sub(by),
        rect.width.saturating_add(by.saturating_mul(2)),
        rect.height.saturating_add(by.saturating_mul(2)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FrameInsets {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl FrameInsets {
    pub(crate) fn between(outer: Rect, visible: Rect) -> Self {
        Self {
            left: visible.x.saturating_sub(outer.x),
            top: visible.y.saturating_sub(outer.y),
            right: edge(outer.x, outer.width).saturating_sub(edge(visible.x, visible.width)),
            bottom: edge(outer.y, outer.height).saturating_sub(edge(visible.y, visible.height)),
        }
    }

    pub(crate) fn outer_for(self, visible: Rect) -> Rect {
        Rect::new(
            visible.x.saturating_sub(self.left),
            visible.y.saturating_sub(self.top),
            visible.width.saturating_add(self.left).saturating_add(self.right),
            visible.height.saturating_add(self.top).saturating_add(self.bottom),
        )
    }
}

fn edge(start: i32, length: i32) -> i32 {
    start.saturating_add(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_measure_invisible_borders() {
        let outer = Rect::new(93, 50, 814, 607);
        let visible = Rect::new(100, 50, 800, 600);
        let insets = FrameInsets::between(outer, visible);
        assert_eq!(insets, FrameInsets { left: 7, top: 0, right: 7, bottom: 7 });
    }

    #[test]
    fn outer_rect_restores_borders_around_target() {
        let insets = FrameInsets { left: 7, top: 0, right: 7, bottom: 7 };
        assert_eq!(insets.outer_for(Rect::new(0, 0, 1280, 800)), Rect::new(-7, 0, 1294, 807));
    }

    #[test]
    fn inflate_grows_every_side() {
        assert_eq!(inflate(Rect::new(10, 20, 100, 50), 3), Rect::new(7, 17, 106, 56));
    }

    #[test]
    fn extreme_values_saturate() {
        let huge = Rect::new(i32::MAX, i32::MIN, i32::MAX, i32::MAX);
        let insets = FrameInsets { left: 7, top: 7, right: 7, bottom: 7 };
        assert_eq!(insets.outer_for(huge).width, i32::MAX);
        assert_eq!(inflate(huge, 3).y, i32::MIN);
    }

    #[test]
    fn win32_rect_converts_to_edges() {
        let rect = RECT { left: -10, top: 5, right: 30, bottom: 25 };
        assert_eq!(rect_from_win32(rect), Rect::new(-10, 5, 40, 20));
    }
}
