use std::f32::consts::TAU;

use deckhand_core::{Point, Rect};

use super::{
    canvas::Canvas,
    color::{BLACK, Premul, WHITE},
    easing::smoothstep,
    palette::Palette,
    pill_paint::paint_text,
    rounded_box::{RoundedBox, coverage},
    scale::{scaled, scaled_px},
    text_mask::TextMask,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BadgeMetrics {
    pub(crate) font_px: i32,
    halo: f32,
    ring: f32,
    reach: i32,
    chip_offset: (i32, i32),
    pad: i32,
    chip_height: i32,
    shadow: i32,
}

impl BadgeMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        Self {
            font_px: scaled_px(12.0, scale),
            halo: scaled(11.0, scale),
            ring: scaled(2.0, scale),
            reach: scaled_px(18.0, scale),
            chip_offset: (scaled_px(14.0, scale), scaled_px(20.0, scale)),
            pad: scaled_px(9.0, scale),
            chip_height: scaled_px(22.0, scale),
            shadow: scaled_px(6.0, scale),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BadgeLayout {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) tip: Point,
    chip: Rect,
    pub(crate) text: Point,
}

impl BadgeLayout {
    pub(crate) fn window_at(&self, tip: Point) -> Rect {
        Rect::new(tip.x.saturating_sub(self.tip.x), tip.y.saturating_sub(self.tip.y), self.width, self.height)
    }
}

pub(crate) fn layout_badge(metrics: &BadgeMetrics, text: (i32, i32)) -> BadgeLayout {
    let tip = Point::new(metrics.reach, metrics.reach);
    let chip_width = text.0 + metrics.pad * 2;
    let chip = Rect::new(tip.x + metrics.chip_offset.0, tip.y + metrics.chip_offset.1, chip_width, metrics.chip_height);
    let text_at = Point::new(chip.x + metrics.pad, chip.y + (metrics.chip_height - text.1) / 2);
    BadgeLayout {
        width: chip.right() + metrics.shadow,
        height: chip.bottom() + metrics.shadow,
        tip,
        chip,
        text: text_at,
    }
}

pub(crate) fn paint_badge(
    canvas: &mut Canvas<'_>,
    layout: &BadgeLayout,
    metrics: &BadgeMetrics,
    palette: &Palette,
    label: &TextMask,
) {
    let chip = RoundedBox::new(
        layout.chip.x as f32,
        layout.chip.y as f32,
        layout.chip.width as f32,
        layout.chip.height as f32,
        layout.chip.height as f32 / 2.0,
    );
    let shadow = chip.shifted(0.0, metrics.shadow as f32 * 0.35);
    let (tip_x, tip_y) = (layout.tip.x as f32, layout.tip.y as f32);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let across = ((px - layout.chip.x as f32) / layout.chip.width.max(1) as f32).clamp(0.0, 1.0);
            let fill = coverage(chip.distance(px, py));
            let shade = (1.0 - smoothstep(-2.0, metrics.shadow as f32, shadow.distance(px, py))) * 0.35 * (1.0 - fill);
            let chip_color = Premul::of(palette.at(across * 0.3).mix(BLACK, 0.08), 0.96 * fill);
            let halo = halo_color(px - tip_x, py - tip_y, metrics, palette);
            canvas.set(x, y, chip_color.over(halo).over(Premul::of(BLACK, shade)).to_bgra());
        }
    }
    paint_text(canvas, label, layout.text, WHITE);
}

fn halo_color(dx: f32, dy: f32, metrics: &BadgeMetrics, palette: &Palette) -> Premul {
    let distance = dx.hypot(dy);
    let off_ring = (distance - metrics.halo).abs() - metrics.ring / 2.0;
    let tone = palette.at(dy.atan2(dx) / TAU);
    let ring = Premul::of(tone.mix(WHITE, 0.3), 0.9 * coverage(off_ring));
    let bloom = Premul::of(tone, 0.4 * (-(off_ring.max(0.0) / (metrics.ring + 2.5)).powi(2)).exp());
    let core = Premul::of(tone, 0.18 * (1.0 - smoothstep(0.0, metrics.halo, distance)));
    ring.over(bloom).over(core)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::canvas::Bitmap;

    fn painted() -> (Bitmap, BadgeLayout) {
        let metrics = BadgeMetrics::at_scale(1.0);
        let layout = layout_badge(&metrics, (40, 15));
        let mut bitmap = Bitmap::new(layout.width as usize, layout.height as usize);
        let label = TextMask { width: 40, height: 15, coverage: vec![255; 40 * 15] };
        paint_badge(&mut bitmap.canvas().unwrap(), &layout, &metrics, &Palette::aurora(), &label);
        (bitmap, layout)
    }

    #[test]
    fn the_chip_sits_below_right_of_the_tip() {
        let (_, layout) = painted();
        assert!(layout.chip.x > layout.tip.x && layout.chip.y > layout.tip.y);
        let window = layout.window_at(Point::new(500, 300));
        assert_eq!((window.x + layout.tip.x, window.y + layout.tip.y), (500, 300));
        assert_eq!((window.width, window.height), (layout.width, layout.height));
    }

    #[test]
    fn the_halo_rings_the_tip_and_the_label_is_white_on_colour() {
        let (bitmap, layout) = painted();
        let ring = bitmap.pixel((layout.tip.x + 11) as usize, layout.tip.y as usize);
        assert!(ring >> 24 > 150);
        let text = bitmap.pixel((layout.text.x + 3) as usize, (layout.text.y + 3) as usize);
        assert_eq!(text, 0xFFFF_FFFF);
        let chip = bitmap.pixel((layout.chip.x + 4) as usize, (layout.chip.y + layout.chip.height / 2) as usize);
        assert!(chip >> 24 > 230 && chip & 0x00FF_FFFF != 0x00FF_FFFF);
        assert!(bitmap.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
    }
}
