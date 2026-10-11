use std::time::Duration;

use deckhand_core::{Point, Rect};

use super::{
    canvas::Canvas,
    color::{Premul, WHITE},
    easing::{ease_out_cubic, progress, smoothstep},
    palette::Palette,
    rounded_box::{RoundedBox, coverage},
    scale::scaled,
};

pub(crate) const CLOSE_IN: Duration = Duration::from_millis(160);
pub(crate) const FLASH_END: Duration = Duration::from_millis(340);
pub(crate) const VIEWFINDER_LIFE: Duration = Duration::from_millis(420);
pub(crate) const STILL_VIEWFINDER_LIFE: Duration = Duration::from_millis(200);
const BRACKET_FADE_START: Duration = Duration::from_millis(250);
const FLASH_RISE_START: Duration = Duration::from_millis(60);
pub(crate) const FLASH_BORDER_ALPHA: f32 = 0.35;
pub(crate) const FLASH_TINT_ALPHA: f32 = 0.06;
const ARM: f32 = 28.0;
const THICKNESS: f32 = 3.0;
const START_OFFSET: f32 = 24.0;
const GLOW: f32 = 6.0;
const FLASH_WIDTH: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

pub(crate) const CORNERS: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomRight, Corner::BottomLeft];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BracketMetrics {
    arm: f32,
    thickness: f32,
    glow: f32,
    start_offset: f32,
}

impl BracketMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        Self {
            arm: scaled(ARM, scale),
            thickness: scaled(THICKNESS, scale).round().max(THICKNESS),
            glow: scaled(GLOW, scale),
            start_offset: scaled(START_OFFSET, scale),
        }
    }

    pub(crate) fn side(&self) -> i32 {
        (self.glow * 2.0 + self.thickness + self.arm).ceil() as i32
    }

    fn inset(&self) -> f32 {
        self.glow + self.thickness
    }
}

pub(crate) fn bracket_offset(elapsed: Duration, metrics: &BracketMetrics, animated: bool) -> f32 {
    if !animated {
        return 0.0;
    }
    metrics.start_offset * (1.0 - ease_out_cubic(progress(elapsed, CLOSE_IN)))
}

pub(crate) fn bracket_alpha(elapsed: Duration, animated: bool) -> f32 {
    if !animated {
        return if elapsed < STILL_VIEWFINDER_LIFE { 1.0 } else { 0.0 };
    }
    1.0 - smoothstep(BRACKET_FADE_START.as_secs_f32(), VIEWFINDER_LIFE.as_secs_f32(), elapsed.as_secs_f32())
}

pub(crate) fn flash_level(elapsed: Duration, animated: bool) -> f32 {
    if !animated {
        return 0.0;
    }
    let seconds = elapsed.as_secs_f32();
    let rise = smoothstep(FLASH_RISE_START.as_secs_f32(), CLOSE_IN.as_secs_f32(), seconds);
    let fall = smoothstep(CLOSE_IN.as_secs_f32(), FLASH_END.as_secs_f32(), seconds);
    rise * (1.0 - fall)
}

pub(crate) fn bracket_window(rect: Rect, corner: Corner, offset: f32, metrics: &BracketMetrics) -> Rect {
    let side = metrics.side();
    let reach = (offset + metrics.inset()).round() as i32;
    let (left, top) = match corner {
        Corner::TopLeft => (rect.x - reach, rect.y - reach),
        Corner::TopRight => (rect.right() + reach - side, rect.y - reach),
        Corner::BottomRight => (rect.right() + reach - side, rect.bottom() + reach - side),
        Corner::BottomLeft => (rect.x - reach, rect.bottom() + reach - side),
    };
    Rect::new(left, top, side, side)
}

pub(crate) fn paint_bracket(canvas: &mut Canvas<'_>, corner: Corner, metrics: &BracketMetrics, palette: &Palette) {
    let start = corner_hue(corner);
    let side = metrics.side() as f32;
    let (glow, thickness, length) = (metrics.glow, metrics.thickness, metrics.thickness + metrics.arm);
    let across = RoundedBox::new(glow, glow, length, thickness, thickness / 2.0);
    let down = RoundedBox::new(glow, glow, thickness, length, thickness / 2.0);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (mut px, mut py) = (x as f32 + 0.5, y as f32 + 0.5);
            if matches!(corner, Corner::TopRight | Corner::BottomRight) {
                px = side - px;
            }
            if matches!(corner, Corner::BottomLeft | Corner::BottomRight) {
                py = side - py;
            }
            let edge = across.distance(px, py).min(down.distance(px, py));
            let halo = (-(edge.max(0.0) / (glow / 2.0).max(1.0)).powi(2)).exp() * 0.75;
            let along = ((px - glow).max(py - glow) / length).clamp(0.0, 1.0);
            let tone = palette.at(start + along * 0.22);
            let color = Premul::of(tone.mix(WHITE, 0.4), coverage(edge)).over(Premul::of(tone, halo));
            canvas.set(x, y, color.to_bgra());
        }
    }
}

fn corner_hue(corner: Corner) -> f32 {
    match corner {
        Corner::TopLeft => 0.0,
        Corner::TopRight => 0.25,
        Corner::BottomRight => 0.5,
        Corner::BottomLeft => 0.75,
    }
}

pub(crate) fn flash_width(scale: f32) -> i32 {
    scaled(FLASH_WIDTH, scale).round().max(2.0) as i32
}

pub(crate) fn paint_flash_band(canvas: &mut Canvas<'_>, origin: Point, rect: Rect, width: i32) {
    let inner = (width as f32).max(1.0);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let px = origin.x as f32 + x as f32 + 0.5;
            let py = origin.y as f32 + y as f32 + 0.5;
            let depth = (px - rect.x as f32)
                .min(rect.right() as f32 - px)
                .min(py - rect.y as f32)
                .min(rect.bottom() as f32 - py);
            let strength = 1.0 - smoothstep(inner * 0.35, inner, depth);
            let alpha = if depth < 0.0 { 0.0 } else { strength };
            canvas.set(x, y, Premul::of(WHITE, alpha).to_bgra());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::canvas::Bitmap;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const RECT: Rect = Rect::new(100, 100, 400, 300);

    #[test]
    fn brackets_close_in_from_twenty_four_pixels_over_160_ms() {
        let metrics = BracketMetrics::at_scale(1.0);
        assert!(crate::overlay::easing::close(bracket_offset(Duration::ZERO, &metrics, true), 24.0));
        assert!(bracket_offset(MS(80), &metrics, true) < 6.0);
        assert!(crate::overlay::easing::close(bracket_offset(MS(160), &metrics, true), 0.0));
        assert!(crate::overlay::easing::close(bracket_offset(Duration::ZERO, &metrics, false), 0.0));
        assert!(crate::overlay::easing::close(
            bracket_offset(Duration::ZERO, &BracketMetrics::at_scale(2.0), true),
            48.0
        ));
    }

    #[test]
    fn bracket_windows_sit_on_their_corners() {
        let metrics = BracketMetrics::at_scale(1.0);
        let (side, inset) = (metrics.side(), metrics.inset() as i32);
        let top_left = bracket_window(RECT, Corner::TopLeft, 0.0, &metrics);
        assert_eq!((top_left.x, top_left.y), (100 - inset, 100 - inset));
        let bottom_right = bracket_window(RECT, Corner::BottomRight, 0.0, &metrics);
        assert_eq!((bottom_right.right(), bottom_right.bottom()), (500 + inset, 400 + inset));
        let far = bracket_window(RECT, Corner::TopRight, 24.0, &metrics);
        assert_eq!((far.right(), far.y, far.width), (500 + inset + 24, 100 - inset - 24, side));
    }

    #[test]
    fn brackets_fade_out_by_420_ms() {
        assert!(crate::overlay::easing::close(bracket_alpha(Duration::ZERO, true), 1.0));
        assert!(crate::overlay::easing::close(bracket_alpha(MS(160), true), 1.0));
        assert!(bracket_alpha(MS(340), true) < 0.6);
        assert!(crate::overlay::easing::close(bracket_alpha(MS(420), true), 0.0));
        assert!(crate::overlay::easing::close(bracket_alpha(MS(150), false), 1.0));
        assert!(crate::overlay::easing::close(bracket_alpha(MS(200), false), 0.0));
    }

    #[test]
    fn the_flash_peaks_at_160_ms_and_is_gone_by_340() {
        assert!(crate::overlay::easing::close(flash_level(Duration::ZERO, true), 0.0));
        assert!(crate::overlay::easing::close(flash_level(MS(160), true), 1.0));
        assert!(flash_level(MS(100), true) > 0.1 && flash_level(MS(250), true) < 0.6);
        assert!(crate::overlay::easing::close(flash_level(MS(340), true), 0.0));
        assert!(crate::overlay::easing::close(flash_level(MS(160), false), 0.0));
    }

    #[test]
    fn the_bracket_is_a_white_l_with_a_glow_mirrored_per_corner() {
        let metrics = BracketMetrics::at_scale(1.0);
        let side = metrics.side() as usize;
        let inset = metrics.inset() as usize;
        let palette = Palette::aurora();
        let mut top_left = Bitmap::new(side, side);
        paint_bracket(&mut top_left.canvas().unwrap(), Corner::TopLeft, &metrics, &palette);
        let mut bottom_right = Bitmap::new(side, side);
        paint_bracket(&mut bottom_right.canvas().unwrap(), Corner::BottomRight, &metrics, &palette);
        assert_eq!(top_left.pixel(inset - 2, inset + 10) >> 24, 255);
        assert_eq!(top_left.pixel(inset + 10, inset - 2) >> 24, 255);
        assert_eq!(top_left.pixel(inset + 10, inset + 10), 0);
        assert_eq!(bottom_right.pixel(side - inset + 1, side - inset - 11) >> 24, 255);
        assert_ne!(top_left.pixel(inset - 2, inset + 10), bottom_right.pixel(side - inset + 1, side - inset - 11));
        assert!(top_left.pixel(inset - 5, inset + 10) >> 24 > 20);
    }

    #[test]
    fn the_flash_band_is_white_at_the_edge_and_fades_inward() {
        let width = flash_width(1.0);
        let mut bitmap = Bitmap::new(20, 1);
        paint_flash_band(&mut bitmap.canvas().unwrap(), Point::new(100, 200), RECT, width);
        assert_eq!(bitmap.pixel(0, 0), 0xFFFF_FFFF);
        assert!(bitmap.pixel(3, 0) >> 24 < 255 && bitmap.pixel(3, 0) >> 24 > 0);
        assert_eq!(bitmap.pixel(10, 0), 0);
    }
}
