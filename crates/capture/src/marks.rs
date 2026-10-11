use deckhand_core::{ImageMapping, Rect};

use crate::canvas::{Bgra, Bounds, Canvas};
use crate::font::{GLYPH_HEIGHT, draw_text, text_width};

const BOX_COLOR: Bgra = [255, 0, 255, 255];
const TAB_COLOR: Bgra = [32, 32, 32, 255];
const TEXT_COLOR: Bgra = [255, 255, 255, 255];
const BOX_THICKNESS: i64 = 2;
const TAB_PADDING: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub rect: Rect,
    pub label: String,
}

pub(crate) fn draw_marks(canvas: &mut Canvas, marks: &[Mark], mapping: ImageMapping, screen: Rect) {
    let reach = drawable_reach(screen, mapping.scale);
    for mark in marks {
        let area = map_to_image(clamp_to(mark.rect, reach), mapping);
        if !canvas.overlaps(area) {
            continue;
        }
        canvas.stroke(area, BOX_THICKNESS, BOX_COLOR);
        draw_label(canvas, &mark.label, area);
    }
}

pub(crate) fn map_to_image(rect: Rect, mapping: ImageMapping) -> Bounds {
    let image = mapping.rect_to_image(rect);
    let (left, top) = (i64::from(image.x), i64::from(image.y));
    let right = i64::from(image.right()).max(left + 1);
    let bottom = i64::from(image.bottom()).max(top + 1);
    Bounds { left, top, right, bottom }
}

fn drawable_reach(screen: Rect, scale: f64) -> Bounds {
    let margin = (scale * (BOX_THICKNESS + 2) as f64).ceil().clamp(1.0, f64::from(i32::MAX)) as i64;
    let limit = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
    let (x, y) = (i64::from(screen.x), i64::from(screen.y));
    Bounds {
        left: limit(x - margin),
        top: limit(y - margin),
        right: limit(x + i64::from(screen.width) + margin),
        bottom: limit(y + i64::from(screen.height) + margin),
    }
}

fn clamp_to(rect: Rect, reach: Bounds) -> Rect {
    let (x, y) = (i64::from(rect.x), i64::from(rect.y));
    let left = x.clamp(reach.left, reach.right);
    let top = y.clamp(reach.top, reach.bottom);
    let right = (x + i64::from(rect.width)).clamp(left, reach.right);
    let bottom = (y + i64::from(rect.height)).clamp(top, reach.bottom);
    Rect::from_edges(left as i32, top as i32, right as i32, bottom as i32)
}

fn draw_label(canvas: &mut Canvas, label: &str, anchor: Bounds) {
    if label.is_empty() {
        return;
    }
    let tab = label_tab(label, anchor, canvas.width());
    canvas.fill(tab, TAB_COLOR);
    draw_text(canvas, label, tab.left + TAB_PADDING, tab.top + TAB_PADDING, TEXT_COLOR);
}

fn label_tab(label: &str, anchor: Bounds, canvas_width: i64) -> Bounds {
    let width = text_width(label) + 2 * TAB_PADDING;
    let height = GLYPH_HEIGHT + 2 * TAB_PADDING;
    let top = if anchor.top >= height { anchor.top - height } else { anchor.top.max(0) };
    let left = anchor.left.min(canvas_width - width).max(0);
    Bounds::sized(left, top, width, height)
}

#[cfg(test)]
mod tests {
    use deckhand_core::Point;

    use super::*;
    use crate::canvas::tests::pixel_at;

    const EMPTY: Bgra = [0, 0, 0, 0];

    fn draw(width: u32, height: u32, marks: &[Mark], origin: Point, scale: f64) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let screen =
            Rect::new(origin.x, origin.y, (f64::from(width) * scale) as i32, (f64::from(height) * scale) as i32);
        let mapping = ImageMapping { origin, scale };
        draw_marks(&mut Canvas::new(&mut pixels, width, height), marks, mapping, screen);
        pixels
    }

    fn mark(x: i32, y: i32, width: i32, height: i32, label: &str) -> Mark {
        Mark { rect: Rect::new(x, y, width, height), label: label.to_string() }
    }

    fn count(pixels: &[u8], color: Bgra) -> usize {
        pixels.as_chunks::<4>().0.iter().filter(|pixel| **pixel == color).count()
    }

    #[test]
    fn maps_screen_rects_through_origin_and_scale() {
        let mapping = ImageMapping { origin: Point::new(-1920, 0), scale: 2.0 };
        let area = map_to_image(Rect::new(-1000, 300, 200, 100), mapping);
        assert_eq!(area, Bounds { left: 460, top: 150, right: 560, bottom: 200 });
        let mapping = ImageMapping { origin: Point::new(0, 0), scale: 4.0 };
        assert_eq!(map_to_image(Rect::new(10, 10, 0, 0), mapping), Bounds { left: 3, top: 3, right: 4, bottom: 4 });
    }

    #[test]
    fn mapping_rounds_like_the_shared_image_mapping() {
        let mapping = ImageMapping { origin: Point::new(7, -3), scale: 1.5 };
        let rect = Rect::new(20, 40, 33, 17);
        let shared = mapping.rect_to_image(rect);
        let area = map_to_image(rect, mapping);
        assert_eq!((area.left, area.top), (i64::from(shared.x), i64::from(shared.y)));
        assert_eq!((area.right, area.bottom), (i64::from(shared.right()), i64::from(shared.bottom())));
    }

    #[test]
    fn extreme_marks_neither_panic_nor_spill() {
        let marks = [
            mark(i32::MAX, i32::MIN, i32::MAX, i32::MAX, "e1"),
            mark(i32::MIN, i32::MIN, i32::MAX, i32::MAX, "e2"),
            mark(-5, -5, i32::MAX, i32::MAX, "e3"),
        ];
        let pixels = draw(50, 50, &marks, Point::new(i32::MIN, 0), 0.001);
        assert_eq!(pixels.len(), 50 * 50 * 4);
        let pixels = draw(50, 50, &marks, Point::new(i32::MAX - 60, i32::MAX - 60), 1.0);
        assert_eq!(pixels.len(), 50 * 50 * 4);
    }

    #[test]
    fn box_is_drawn_in_magenta_at_the_mapped_position() {
        let pixels = draw(100, 100, &[mark(140, 140, 80, 80, "")], Point::new(100, 100), 2.0);
        assert_eq!(pixel_at(&pixels, 100, 20, 20), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 21, 30), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 59, 59), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 30, 30), EMPTY);
        assert_eq!(pixel_at(&pixels, 100, 19, 19), EMPTY);
        assert_eq!(pixel_at(&pixels, 100, 60, 60), EMPTY);
    }

    #[test]
    fn label_sits_above_the_box_when_there_is_room() {
        let pixels = draw(100, 100, &[mark(10, 40, 30, 30, "e1")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 10, 22), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10, 39), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10, 21), EMPTY);
        assert_eq!(pixel_at(&pixels, 100, 10, 40), BOX_COLOR);
    }

    #[test]
    fn label_moves_inside_at_the_top_edge() {
        let pixels = draw(100, 100, &[mark(10, 0, 50, 50, "w3")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 10, 0), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10, 17), TAB_COLOR);
    }

    #[test]
    fn label_of_a_box_starting_above_the_image_is_clamped_to_the_top() {
        let pixels = draw(100, 100, &[mark(10, -50, 30, 100, "e1")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 10, 0), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10, 17), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10, 30), BOX_COLOR);
    }

    #[test]
    fn label_of_a_box_starting_left_of_the_image_is_clamped_to_the_left() {
        let pixels = draw(100, 100, &[mark(-20, 50, 40, 10, "e1")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 0, 32), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 5, 50), BOX_COLOR);
    }

    #[test]
    fn label_text_is_white_on_the_tab() {
        let pixels = draw(100, 100, &[mark(10, 40, 30, 30, "1")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 10 + 2 + 4, 22 + 2), TEXT_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 10 + 2, 22 + 2), TAB_COLOR);
    }

    #[test]
    fn label_shifts_left_to_stay_visible() {
        let pixels = draw(100, 100, &[mark(95, 50, 5, 5, "e12")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 99, 40), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 100 - 38, 40), TAB_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 100 - 39, 40), EMPTY);
    }

    #[test]
    fn marks_wholly_outside_the_image_draw_nothing_at_all() {
        let marks = [
            mark(5000, 40, 10, 10, "e1"),
            mark(-500, 40, 10, 10, "e2"),
            mark(40, 5000, 10, 10, "e3"),
            mark(40, -500, 10, 10, "e4"),
            mark(100, 100, 10, 10, "e5"),
        ];
        let pixels = draw(100, 100, &marks, Point::new(0, 0), 1.0);
        assert_eq!((count(&pixels, BOX_COLOR), count(&pixels, TAB_COLOR)), (0, 0));
    }

    #[test]
    fn partly_visible_marks_are_clipped_without_false_edges() {
        let pixels = draw(100, 100, &[mark(-10, 90, 30, 30, "")], Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 0, 90), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 19, 99), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 0, 95), EMPTY);
        assert_eq!(pixel_at(&pixels, 100, 10, 99), EMPTY);
        assert_eq!(pixel_at(&pixels, 100, 50, 50), EMPTY);
    }
}
