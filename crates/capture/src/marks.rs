use deckhand_core::{Point, Rect};

use crate::canvas::{Bgra, Bounds, Canvas};
use crate::font::{GLYPH_HEIGHT, draw_text, text_width};

const BOX_COLOR: Bgra = [255, 0, 255, 255];
const TAB_COLOR: Bgra = [32, 32, 32, 255];
const TEXT_COLOR: Bgra = [255, 255, 255, 255];
const BOX_THICKNESS: i64 = 2;
const TAB_PADDING: i64 = 2;
const COORDINATE_LIMIT: f64 = 1_073_741_824.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub rect: Rect,
    pub label: String,
}

pub(crate) fn draw_marks(canvas: &mut Canvas, marks: &[Mark], origin: Point, scale: f64) {
    for mark in marks {
        let area = map_to_image(mark.rect, origin, scale);
        canvas.stroke(area, BOX_THICKNESS, BOX_COLOR);
        draw_label(canvas, &mark.label, area);
    }
}

pub(crate) fn map_to_image(rect: Rect, origin: Point, scale: f64) -> Bounds {
    let to_image = |screen: f64, origin: i32| {
        ((screen - f64::from(origin)) / scale).round().clamp(-COORDINATE_LIMIT, COORDINATE_LIMIT) as i64
    };
    let left = to_image(f64::from(rect.x), origin.x);
    let top = to_image(f64::from(rect.y), origin.y);
    let right = to_image(f64::from(rect.x) + f64::from(rect.width), origin.x).max(left + 1);
    let bottom = to_image(f64::from(rect.y) + f64::from(rect.height), origin.y).max(top + 1);
    Bounds { left, top, right, bottom }
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
    let top = if anchor.top >= height { anchor.top - height } else { anchor.top };
    let left = anchor.left.min(canvas_width - width).max(0);
    Bounds::sized(left, top, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::tests::pixel_at;

    const EMPTY: Bgra = [0, 0, 0, 0];

    fn draw(width: u32, height: u32, marks: &[Mark], origin: Point, scale: f64) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        draw_marks(&mut Canvas::new(&mut pixels, width, height), marks, origin, scale);
        pixels
    }

    fn mark(x: i32, y: i32, width: i32, height: i32, label: &str) -> Mark {
        Mark { rect: Rect::new(x, y, width, height), label: label.to_string() }
    }

    #[test]
    fn maps_screen_rects_through_origin_and_scale() {
        let area = map_to_image(Rect::new(-1000, 300, 200, 100), Point::new(-1920, 0), 2.0);
        assert_eq!(area, Bounds { left: 460, top: 150, right: 560, bottom: 200 });
        let area = map_to_image(Rect::new(10, 10, 0, 0), Point::new(0, 0), 4.0);
        assert_eq!(area, Bounds { left: 3, top: 3, right: 4, bottom: 4 });
    }

    #[test]
    fn mapping_clamps_extreme_values() {
        let area = map_to_image(Rect::new(i32::MAX, i32::MIN, i32::MAX, i32::MAX), Point::new(i32::MIN, 0), 0.001);
        assert!(area.right <= COORDINATE_LIMIT as i64 + 1);
        assert!(area.top >= -(COORDINATE_LIMIT as i64));
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
    fn marks_outside_the_image_are_clipped() {
        let marks = [mark(-500, -500, 450, 450, "e1"), mark(5000, 5000, 10, 10, "e2"), mark(-10, 90, 30, 30, "e3")];
        let pixels = draw(100, 100, &marks, Point::new(0, 0), 1.0);
        assert_eq!(pixel_at(&pixels, 100, 0, 90), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 19, 99), BOX_COLOR);
        assert_eq!(pixel_at(&pixels, 100, 50, 50), EMPTY);
    }
}
