use crate::canvas::{Bgra, Bounds, Canvas};

const GLYPH_COLUMNS: i64 = 5;
const GLYPH_ROWS: i64 = 7;
const SCALE: i64 = 2;
const SPACING: i64 = SCALE;
const GLYPH_WIDTH: i64 = GLYPH_COLUMNS * SCALE;
pub(crate) const GLYPH_HEIGHT: i64 = GLYPH_ROWS * SCALE;
const ADVANCE: i64 = GLYPH_WIDTH + SPACING;

type Glyph = [u8; GLYPH_ROWS as usize];

const DIGITS: [Glyph; 10] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
];
const LOWER_E: Glyph = [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E];
const LOWER_I: Glyph = [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E];
const LOWER_W: Glyph = [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A];
const DASH: Glyph = [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00];
const BLANK: Glyph = [0; GLYPH_ROWS as usize];

fn glyph(character: char) -> Option<Glyph> {
    match character {
        '0'..='9' => character.to_digit(10).map(|digit| DIGITS[digit as usize]),
        'e' => Some(LOWER_E),
        'i' => Some(LOWER_I),
        'w' => Some(LOWER_W),
        '-' => Some(DASH),
        ' ' => Some(BLANK),
        _ => None,
    }
}

pub(crate) fn text_width(text: &str) -> i64 {
    let count = text.chars().count() as i64;
    if count == 0 { 0 } else { count * ADVANCE - SPACING }
}

pub(crate) fn draw_text(canvas: &mut Canvas, text: &str, left: i64, top: i64, color: Bgra) {
    let mut x = left;
    for character in text.chars() {
        if x >= canvas.width() {
            return;
        }
        match glyph(character) {
            Some(rows) => draw_glyph(canvas, rows, x, top, color),
            None => canvas.fill(Bounds::sized(x, top, GLYPH_WIDTH, GLYPH_HEIGHT), color),
        }
        x += ADVANCE;
    }
}

fn draw_glyph(canvas: &mut Canvas, rows: Glyph, left: i64, top: i64, color: Bgra) {
    for (row, bits) in (0..GLYPH_ROWS).zip(rows.iter()) {
        for column in 0..GLYPH_COLUMNS {
            if bits & (0x10 >> column) != 0 {
                canvas.fill(Bounds::sized(left + column * SCALE, top + row * SCALE, SCALE, SCALE), color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::tests::pixel_at;

    const WHITE: Bgra = [255, 255, 255, 255];
    const EMPTY: Bgra = [0, 0, 0, 0];

    fn render(text: &str, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        draw_text(&mut Canvas::new(&mut pixels, width, height), text, 0, 0, WHITE);
        pixels
    }

    #[test]
    fn digit_one_is_drawn_at_double_scale() {
        let pixels = render("1", 10, 14);
        assert_eq!(pixel_at(&pixels, 10, 4, 0), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 5, 1), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 0, 0), EMPTY);
        assert_eq!(pixel_at(&pixels, 10, 2, 2), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 2, 13), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 0, 13), EMPTY);
        assert_eq!(pixel_at(&pixels, 10, 9, 6), EMPTY);
    }

    #[test]
    fn letters_have_their_shapes() {
        let pixels = render("e", 10, 14);
        assert_eq!(pixel_at(&pixels, 10, 4, 0), EMPTY);
        assert_eq!(pixel_at(&pixels, 10, 0, 8), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 9, 8), WHITE);
        let pixels = render("i", 10, 14);
        assert_eq!(pixel_at(&pixels, 10, 4, 0), WHITE);
        assert_eq!(pixel_at(&pixels, 10, 4, 2), EMPTY);
    }

    #[test]
    fn unknown_characters_are_filled_boxes() {
        let pixels = render("?", 10, 14);
        assert!(pixels.as_chunks::<4>().0.iter().all(|pixel| *pixel == WHITE));
    }

    #[test]
    fn spaces_draw_nothing_and_advance() {
        let pixels = render(" 1", 24, 14);
        assert!((0..12).all(|x| (0..14).all(|y| pixel_at(&pixels, 24, x, y) == EMPTY)));
        assert_eq!(pixel_at(&pixels, 24, 16, 0), WHITE);
    }

    #[test]
    fn every_required_character_has_a_glyph() {
        assert!("0123456789ewi -".chars().all(|character| glyph(character).is_some()));
        assert_eq!(text_width(""), 0);
        assert_eq!(text_width("e12"), 3 * 12 - 2);
    }

    #[test]
    fn text_is_clipped_at_the_canvas_edge() {
        let pixels = render("e1234567890", 16, 8);
        assert_eq!(pixels.len(), 16 * 8 * 4);
    }
}
