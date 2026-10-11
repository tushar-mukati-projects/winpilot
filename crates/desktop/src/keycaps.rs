use deckhand_core::{Point, Rect};

use crate::{
    canvas::Canvas,
    easing::smoothstep,
    overlay_color::{Premul, Rgb},
    overlay_palette::{BLACK, WHITE},
    overlay_scale::scaled_px,
    palette::Palette,
    pill_paint::paint_text,
    rounded_box::{RoundedBox, coverage},
    text_mask::TextMask,
};

const CAP_FILL: Rgb = Rgb::new(30, 30, 38);
const BACKDROP: Rgb = Rgb::new(12, 12, 16);
const BACKDROP_ALPHA: f32 = 0.62;
const PLUS: Rgb = Rgb::new(225, 225, 235);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChipText {
    Keys(Vec<Vec<String>>),
    Typing(usize),
}

impl ChipText {
    pub(crate) fn items(&self) -> Vec<Item> {
        match self {
            Self::Typing(count) => {
                let unit = if *count == 1 { "char" } else { "chars" };
                vec![Item::Cap(format!("typing\u{2026} {count} {unit}"))]
            }
            Self::Keys(chords) => {
                let mut items = Vec::new();
                for (index, chord) in chords.iter().enumerate() {
                    if index > 0 {
                        items.push(Item::Gap);
                    }
                    for (key_index, key) in chord.iter().enumerate() {
                        if key_index > 0 {
                            items.push(Item::Plus);
                        }
                        items.push(Item::Cap(key.clone()));
                    }
                }
                items
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    Cap(String),
    Plus,
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CapMetrics {
    pub(crate) font_px: i32,
    height: i32,
    pad: i32,
    plus_room: i32,
    gap: i32,
    shadow: i32,
    tray: i32,
    pub(crate) lift: i32,
}

impl CapMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        Self {
            font_px: scaled_px(13.0, scale),
            height: scaled_px(30.0, scale),
            pad: scaled_px(10.0, scale),
            plus_room: scaled_px(18.0, scale),
            gap: scaled_px(12.0, scale),
            shadow: scaled_px(10.0, scale),
            tray: scaled_px(6.0, scale),
            lift: scaled_px(28.0, scale),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Placed {
    Cap { rect: Rect, text: Point, mask: usize },
    Plus { text: Point },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapsLayout {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) tray: Rect,
    pub(crate) placed: Vec<Placed>,
}

impl CapsLayout {
    pub(crate) fn window_in(&self, rect: Rect, metrics: &CapMetrics) -> Rect {
        let x = rect.center().x - self.width / 2;
        let y = rect.bottom() - metrics.lift - self.height + metrics.shadow;
        Rect::new(x, y, self.width, self.height)
    }
}

pub(crate) fn layout_caps(items: &[Item], masks: &[TextMask], plus: &TextMask, metrics: &CapMetrics) -> CapsLayout {
    let mut x = metrics.shadow + metrics.tray;
    let top = metrics.shadow + metrics.tray;
    let mut placed = Vec::new();
    let mut next_mask = 0;
    for item in items {
        match item {
            Item::Cap(_) => {
                let mask = masks.get(next_mask);
                let (width, height) = mask.map_or((0, 0), |mask| (mask.width as i32, mask.height as i32));
                let rect = Rect::new(x, top, width + metrics.pad * 2, metrics.height);
                placed.push(Placed::Cap {
                    rect,
                    text: Point::new(x + metrics.pad, top + (metrics.height - height) / 2),
                    mask: next_mask,
                });
                next_mask += 1;
                x = rect.right();
            }
            Item::Plus => {
                let text_x = x + (metrics.plus_room - plus.width as i32) / 2;
                placed.push(Placed::Plus { text: Point::new(text_x, top + (metrics.height - plus.height as i32) / 2) });
                x += metrics.plus_room;
            }
            Item::Gap => x += metrics.gap,
        }
    }
    let tray =
        Rect::new(metrics.shadow, metrics.shadow, x + metrics.tray - metrics.shadow, metrics.height + metrics.tray * 2);
    CapsLayout { width: tray.right() + metrics.shadow, height: tray.bottom() + metrics.shadow, tray, placed }
}

pub(crate) fn paint_caps(
    canvas: &mut Canvas<'_>,
    layout: &CapsLayout,
    masks: &[TextMask],
    plus: &TextMask,
    palette: &Palette,
) {
    let caps: Vec<Rect> = layout
        .placed
        .iter()
        .filter_map(|placed| match placed {
            Placed::Cap { rect, .. } => Some(*rect),
            Placed::Plus { .. } => None,
        })
        .collect();
    paint_tray(canvas, layout.tray);
    for (index, cap) in caps.iter().enumerate() {
        let tint = palette.at(index as f32 / caps.len().max(1) as f32 * 0.4);
        paint_cap(canvas, *cap, tint);
    }
    for placed in &layout.placed {
        match placed {
            Placed::Cap { text, mask, .. } => {
                if let Some(mask) = masks.get(*mask) {
                    paint_text(canvas, mask, *text, WHITE);
                }
            }
            Placed::Plus { text } => paint_text(canvas, plus, *text, PLUS),
        }
    }
}

fn paint_tray(canvas: &mut Canvas<'_>, tray: Rect) {
    let radius = tray.height as f32 * 0.32;
    let body = RoundedBox::new(tray.x as f32, tray.y as f32, tray.width as f32, tray.height as f32, radius);
    let shadow = body.shifted(0.0, 3.0);
    let blur = (tray.x as f32).max(1.0);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let edge = body.distance(px, py);
            let fill = coverage(edge);
            let rim = (fill - coverage(edge + 1.0)).max(0.0);
            let shade = 0.28 * (1.0 - smoothstep(-blur * 0.3, blur, shadow.distance(px, py))) * (1.0 - fill);
            let color = Premul::of(WHITE, 0.12 * rim)
                .over(Premul::of(BACKDROP, BACKDROP_ALPHA * fill))
                .over(Premul::of(BLACK, shade));
            canvas.set(x, y, color.to_bgra());
        }
    }
}

fn paint_cap(canvas: &mut Canvas<'_>, cap: Rect, tint: Rgb) {
    let radius = cap.height as f32 * 0.28;
    let body = RoundedBox::new(cap.x as f32, cap.y as f32, cap.width as f32, cap.height as f32, radius);
    let shadow = body.shifted(0.0, 2.5);
    let height = cap.height as f32;
    let area = Rect::new(cap.x - 8, cap.y - 6, cap.width + 16, cap.height + 16);
    for y in area.y.max(0)..area.bottom() {
        for x in area.x.max(0)..area.right() {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let edge = body.distance(px, py);
            let fill = coverage(edge);
            let depth = (py - cap.y as f32) / height;
            let rim = (fill - coverage(edge + 1.0)).max(0.0);
            let sheen = 0.16 * (1.0 - smoothstep(0.0, 0.5, depth)) * fill;
            let lip = 0.35 * smoothstep(0.82, 1.0, depth) * fill;
            let shade = 0.3 * (1.0 - smoothstep(-1.0, 6.0, shadow.distance(px, py))) * (1.0 - fill);
            let color = Premul::of(tint.mix(WHITE, 0.55), 0.55 * rim)
                .over(Premul::of(WHITE, sheen))
                .over(Premul::of(BLACK, lip))
                .over(Premul::of(CAP_FILL, 0.9 * fill))
                .over(Premul::of(BLACK, shade));
            canvas.blend(x as usize, y as usize, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Bitmap;

    fn keys() -> ChipText {
        ChipText::Keys(vec![vec!["Ctrl".to_string(), "S".to_string()], vec!["Enter".to_string()]])
    }

    #[test]
    fn chords_become_caps_with_plus_signs_and_gaps() {
        assert_eq!(
            keys().items(),
            vec![
                Item::Cap("Ctrl".to_string()),
                Item::Plus,
                Item::Cap("S".to_string()),
                Item::Gap,
                Item::Cap("Enter".to_string())
            ]
        );
    }

    #[test]
    fn typing_never_shows_the_text_only_the_count() {
        assert_eq!(ChipText::Typing(42).items(), vec![Item::Cap("typing\u{2026} 42 chars".to_string())]);
        assert_eq!(ChipText::Typing(1).items(), vec![Item::Cap("typing\u{2026} 1 char".to_string())]);
    }

    #[test]
    fn caps_lay_out_left_to_right_and_sit_above_the_bottom_centre() {
        let metrics = CapMetrics::at_scale(1.0);
        let mask = |width: usize| TextMask { width, height: 16, coverage: vec![255; width * 16] };
        let masks = vec![mask(28), mask(9), mask(38)];
        let layout = layout_caps(&keys().items(), &masks, &mask(8), &metrics);
        assert_eq!(layout.placed.len(), 4);
        let rects: Vec<Rect> = layout
            .placed
            .iter()
            .filter_map(|placed| match placed {
                Placed::Cap { rect, .. } => Some(*rect),
                Placed::Plus { .. } => None,
            })
            .collect();
        assert!(rects.windows(2).all(|pair| pair[1].x > pair[0].right()));
        let window = layout.window_in(Rect::new(0, 0, 1000, 800), &metrics);
        assert!((window.center().x - 500).abs() <= 1);
        assert!(window.bottom() <= 800 - metrics.lift + metrics.shadow);
        let mut bitmap = Bitmap::new(layout.width as usize, layout.height as usize);
        paint_caps(&mut bitmap.canvas().unwrap(), &layout, &masks, &mask(8), &Palette::aurora());
        let first = rects[0];
        assert!(bitmap.pixel((first.x + 3) as usize, (first.y + first.height / 2) as usize) >> 24 > 200);
        assert!(bitmap.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
    }
}
