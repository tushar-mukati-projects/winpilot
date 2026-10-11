use deckhand_core::Point;

use crate::{
    canvas::{Bitmap, Canvas},
    easing::{lerp, smoothstep},
    overlay_color::{Premul, Rgb},
    overlay_palette::{BLACK, WHITE},
    palette::Palette,
    pill_layout::PillLayout,
    rounded_box::{RoundedBox, coverage},
    text_mask::TextMask,
};

const SHADOW_ALPHA: f32 = 0.32;
const SHEEN_ALPHA: f32 = 0.08;
const RIM_ALPHA: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PillStyle {
    pub(crate) fill: Rgb,
    pub(crate) fill_alpha: f32,
    pub(crate) lead: Rgb,
    pub(crate) hint: Rgb,
}

pub(crate) const GLASS_PILL: PillStyle = PillStyle {
    fill: Rgb::new(16, 16, 20),
    fill_alpha: 0.82,
    lead: Rgb::new(255, 255, 255),
    hint: Rgb::new(200, 200, 210),
};

pub(crate) struct PillArt<'a> {
    pub(crate) layout: PillLayout,
    pub(crate) style: PillStyle,
    pub(crate) lead: &'a TextMask,
    pub(crate) hint: Option<&'a TextMask>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RimPixel {
    x: usize,
    y: usize,
    cover: f32,
    around: f32,
    base: Premul,
}

pub(crate) fn paint_pill(canvas: &mut Canvas<'_>, art: &PillArt<'_>) {
    paint_body(canvas, &art.layout, &art.style);
    let text = art.layout.text;
    paint_text(canvas, art.lead, text, art.style.lead);
    if let Some(hint) = art.hint {
        paint_text(canvas, hint, Point::new(text.x + art.lead.width as i32, text.y), art.style.hint);
    }
}

fn body_box(layout: &PillLayout) -> RoundedBox {
    let body = layout.body;
    RoundedBox::new(body.x as f32, body.y as f32, body.width as f32, body.height as f32, body.height as f32 / 2.0)
}

fn paint_body(canvas: &mut Canvas<'_>, layout: &PillLayout, style: &PillStyle) {
    let body = body_box(layout);
    let blur = layout.body.x as f32 * 0.8;
    let shadow = body.shifted(0.0, blur * 0.25);
    let (top, height) = (layout.body.y as f32, layout.body.height as f32);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let shade = 1.0 - smoothstep(-blur * 0.4, blur, shadow.distance(px, py));
            let fill = coverage(body.distance(px, py));
            let sheen = SHEEN_ALPHA * (1.0 - smoothstep(0.0, height * 0.55, py - top)) * fill;
            let color = Premul::of(WHITE, sheen)
                .over(Premul::of(style.fill, style.fill_alpha * fill))
                .over(Premul::of(BLACK, SHADOW_ALPHA * shade * shade * (1.0 - fill)));
            canvas.set(x, y, color.to_bgra());
        }
    }
}

pub(crate) fn paint_text(canvas: &mut Canvas<'_>, mask: &TextMask, at: Point, color: Rgb) {
    for y in 0..mask.height {
        for x in 0..mask.width {
            let cover = mask.at(x, y);
            let (Ok(cx), Ok(cy)) = (usize::try_from(at.x + x as i32), usize::try_from(at.y + y as i32)) else {
                continue;
            };
            if cover > 0 {
                canvas.blend(cx, cy, Premul::of(color, f32::from(cover) / 255.0));
            }
        }
    }
}

pub(crate) fn pill_rim(layout: &PillLayout) -> Vec<RimPixel> {
    let body = body_box(layout);
    let (left, width) = (layout.body.x as f32, layout.body.width.max(1) as f32);
    let middle = layout.body.y as f32 + layout.body.height as f32 / 2.0;
    let mut rim = Vec::new();
    for y in 0..layout.window.height.max(0) as usize {
        for x in 0..layout.window.width.max(0) as usize {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let edge = body.distance(px, py);
            let line = (coverage(edge) - coverage(edge + 1.2)).max(0.0);
            let halo = if edge > 0.0 { 0.28 * (-(edge / 1.6).powi(2)).exp() } else { 0.0 };
            let cover = line.max(halo);
            if cover > 0.03 {
                let across = ((px - left) / width).clamp(0.0, 1.0) * 0.5;
                let around = if py < middle { across } else { 1.0 - across };
                rim.push(RimPixel { x, y, cover, around, base: Premul::default() });
            }
        }
    }
    rim
}

pub(crate) fn rest_rim_on(rim: &mut [RimPixel], base: &Bitmap) {
    for pixel in rim {
        pixel.base = Premul::from_bgra(base.pixel(pixel.x, pixel.y));
    }
}

pub(crate) fn paint_rim(canvas: &mut Canvas<'_>, rim: &[RimPixel], palette: &Palette, spin: f32) {
    for pixel in rim {
        let color = palette.at(pixel.around - spin).mix(WHITE, 0.2);
        canvas.set(pixel.x, pixel.y, Premul::of(color, RIM_ALPHA * pixel.cover).over(pixel.base).to_bgra());
    }
}

pub(crate) fn paint_sparkle(canvas: &mut Canvas<'_>, layout: &PillLayout, palette: &Palette, phase: f32, pulse: f32) {
    let region = layout.glyph_region();
    let (cx, cy) = layout.glyph;
    let radius = layout.glyph_radius * lerp(0.78, 1.0, pulse);
    let core = palette.at(phase).mix(WHITE, 0.45);
    let glow = palette.at(phase + 0.2);
    for y in region.y.max(0)..region.bottom() {
        for x in region.x.max(0)..region.right() {
            let (dx, dy) = ((x as f32 + 0.5 - cx).abs(), (y as f32 + 0.5 - cy).abs());
            let star = (dx.sqrt() + dy.sqrt()).powi(2) - radius;
            let shape = coverage(star * 0.9);
            let distance = dx.hypot(dy);
            let halo = lerp(0.25, 0.55, pulse) * (-(distance / (radius * 0.9)).powi(2)).exp();
            let shine = Premul::of(core, shape).over(Premul::of(glow, halo));
            canvas.blend(x as usize, y as usize, shine);
        }
    }
}

pub(crate) fn paint_dot(canvas: &mut Canvas<'_>, layout: &PillLayout, color: Rgb, pulse: f32) {
    let region = layout.glyph_region();
    let (cx, cy) = layout.glyph;
    let radius = layout.glyph_radius * 0.62;
    let halo_radius = radius * lerp(1.3, 2.6, pulse);
    for y in region.y.max(0)..region.bottom() {
        for x in region.x.max(0)..region.right() {
            let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
            let halo = lerp(0.0, 0.4, pulse) * (1.0 - smoothstep(radius * 0.6, halo_radius, distance));
            let core = lerp(0.75, 1.0, pulse) * coverage(distance - radius);
            canvas.blend(x as usize, y as usize, Premul::of(color, core).over(Premul::of(color, halo)));
        }
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::{
        canvas::Bitmap,
        pill_layout::{Align, PillMetrics, layout_pill},
    };

    fn layout() -> PillLayout {
        let metrics = PillMetrics::at_scale(1.0);
        layout_pill(Rect::new(0, 0, 800, 600), &metrics, Align::TopCenter, (60, 16), Rect::new(0, 0, 1920, 1080))
    }

    fn painted(pulse: f32) -> Bitmap {
        let layout = layout();
        let mut bitmap = Bitmap::new(layout.window.width as usize, layout.window.height as usize);
        let lead = TextMask { width: 60, height: 16, coverage: vec![255; 60 * 16] };
        let art = PillArt { layout, style: GLASS_PILL, lead: &lead, hint: None };
        paint_pill(&mut bitmap.canvas().unwrap(), &art);
        let mut rim = pill_rim(&layout);
        rest_rim_on(&mut rim, &bitmap);
        let mut canvas = bitmap.canvas().unwrap();
        paint_rim(&mut canvas, &rim, &Palette::aurora(), 0.3);
        paint_sparkle(&mut canvas, &layout, &Palette::aurora(), 0.1, pulse);
        bitmap
    }

    #[test]
    fn the_body_is_dark_glass() {
        let layout = layout();
        let bitmap = painted(0.0);
        let pixel = bitmap.pixel((layout.text.x - 3) as usize, (layout.body.y + layout.body.height - 6) as usize);
        assert_eq!(pixel >> 24, (0.82f32 * 255.0).round() as u32);
        assert!((pixel >> 16) & 0xFF < 30);
    }

    #[test]
    fn text_is_light_and_the_pill_is_premultiplied() {
        let layout = layout();
        let bitmap = painted(1.0);
        let text = bitmap.pixel((layout.text.x + 5) as usize, (layout.text.y + 5) as usize);
        assert!((text >> 8) & 0xFF > 230);
        for pixel in &bitmap.pixels {
            assert!((0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24));
        }
    }

    #[test]
    fn the_rim_is_coloured_and_changes_as_it_spins() {
        let layout = layout();
        let rim = pill_rim(&layout);
        assert!(rim.len() > layout.body.width as usize * 2);
        let probe = (layout.window.width as usize / 2, layout.body.y as usize);
        let paint = |spin: f32| {
            let mut bitmap = Bitmap::new(layout.window.width as usize, layout.window.height as usize);
            paint_rim(&mut bitmap.canvas().unwrap(), &rim, &Palette::aurora(), spin);
            bitmap.pixel(probe.0, probe.1)
        };
        assert!(paint(0.0) >> 24 > 150);
        assert_ne!(paint(0.0), paint(0.4));
    }

    #[test]
    fn the_sparkle_pulses() {
        let layout = layout();
        let (x, y) = (layout.glyph.0 as usize, layout.glyph.1 as usize);
        let tip = x + (layout.glyph_radius * 0.85) as usize;
        let (small, large) = (painted(0.0), painted(1.0));
        assert!(small.pixel(x, y) >> 24 > 240);
        assert!(large.pixel(tip, y) >> 24 > small.pixel(tip, y) >> 24);
    }

    #[test]
    fn corners_of_the_window_are_clear() {
        assert_eq!(painted(1.0).pixel(0, 0), 0);
    }
}
