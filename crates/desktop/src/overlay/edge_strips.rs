use deckhand_core::Rect;

use crate::frame::inflate;

pub(crate) const EDGE_COUNT: usize = 4;

pub(crate) fn border_strips(target: Rect, thickness: i32, screen: Rect) -> [Option<Rect>; EDGE_COUNT] {
    let outline = inflate(target, thickness);
    let inner_height = outline.height.saturating_sub(thickness.saturating_mul(2));
    let below_top = outline.y.saturating_add(thickness);
    let edges = [
        Rect::new(outline.x, outline.y, outline.width, thickness),
        Rect::new(outline.x, far_edge(outline.y, outline.height, thickness), outline.width, thickness),
        Rect::new(outline.x, below_top, thickness, inner_height),
        Rect::new(far_edge(outline.x, outline.width, thickness), below_top, thickness, inner_height),
    ];
    edges.map(|edge| edge.intersection(screen))
}

fn far_edge(start: i32, length: i32, thickness: i32) -> i32 {
    start.saturating_add(length).saturating_sub(thickness)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 2560, 1600);

    fn area(strips: &[Option<Rect>]) -> i64 {
        strips.iter().flatten().map(|strip| strip.area()).sum()
    }

    #[test]
    fn four_strips_frame_the_target_without_overlap() {
        let strips = border_strips(Rect::new(100, 200, 400, 300), 3, SCREEN);
        assert_eq!(
            strips,
            [
                Some(Rect::new(97, 197, 406, 3)),
                Some(Rect::new(97, 500, 406, 3)),
                Some(Rect::new(97, 200, 3, 300)),
                Some(Rect::new(500, 200, 3, 300)),
            ]
        );
        assert_eq!(area(&strips), 406 * 306 - 400 * 300);
    }

    #[test]
    fn memory_grows_with_the_perimeter_not_the_area() {
        let strips = border_strips(Rect::new(10, 10, 2000, 1500), 3, SCREEN);
        assert!(area(&strips) < 3 * 2 * (2006 + 1506));
    }

    #[test]
    fn strips_are_clipped_to_the_virtual_screen() {
        let strips = border_strips(Rect::new(-50, 1500, 300, 400), 3, SCREEN);
        assert_eq!(strips[0], Some(Rect::new(0, 1497, 253, 3)));
        assert_eq!(strips[1], None);
        assert_eq!(strips[2], None);
        assert_eq!(strips[3], Some(Rect::new(250, 1500, 3, 100)));
    }

    #[test]
    fn negative_screen_origins_are_respected() {
        let screen = Rect::new(-1920, 0, 4480, 1600);
        let strips = border_strips(Rect::new(-1930, 10, 100, 100), 3, screen);
        assert_eq!(strips[0], Some(Rect::new(-1920, 7, 93, 3)));
        assert_eq!(strips[2], None);
    }

    #[test]
    fn target_off_screen_has_no_strips() {
        assert_eq!(border_strips(Rect::new(5000, 5000, 100, 100), 3, SCREEN), [None; 4]);
    }

    #[test]
    fn extreme_rects_do_not_overflow() {
        let strips = border_strips(Rect::new(i32::MAX - 5, i32::MIN, i32::MAX, i32::MAX), 3, SCREEN);
        assert!(strips.iter().flatten().all(|strip| !strip.is_empty()));
        let _degenerate = border_strips(Rect::new(0, 0, 0, 0), 3, SCREEN);
    }
}
