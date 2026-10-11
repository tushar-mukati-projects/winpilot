use std::time::Duration;

use deckhand_core::{Point, Rect};

use super::{
    canvas::Canvas,
    color::{Premul, WHITE},
    easing::{ease_in_out_cubic, progress, smoothstep},
    palette::Palette,
    scale::{scaled, scaled_px},
};

pub(crate) const SCAN_SWEEP: Duration = Duration::from_millis(420);
pub(crate) const STILL_SCAN: Duration = Duration::from_millis(300);
pub(crate) const SCAN_COOLDOWN: Duration = Duration::from_millis(1500);
const BAND_ALPHA: f32 = 0.18;
const BAND_HEIGHT: f32 = 60.0;
const EDGE_ALPHA: f32 = 0.55;
const STILL_EDGE_ALPHA: f32 = 0.35;

pub(crate) fn band_height(scale: f32) -> i32 {
    scaled_px(BAND_HEIGHT, scale).max(8)
}

pub(crate) fn edge_width(scale: f32) -> i32 {
    scaled_px(3.0, scale).max(2)
}

pub(crate) fn band_top(rect: Rect, elapsed: Duration, scale: f32) -> i32 {
    let height = band_height(scale);
    let travel = ease_in_out_cubic(progress(elapsed, SCAN_SWEEP));
    rect.y.saturating_sub(height / 2).saturating_add((travel * rect.height as f32).round() as i32)
}

pub(crate) fn edge_alpha(elapsed: Duration, animated: bool) -> f32 {
    if !animated {
        return if elapsed < STILL_SCAN { STILL_EDGE_ALPHA } else { 0.0 };
    }
    let done = progress(elapsed, SCAN_SWEEP);
    EDGE_ALPHA * (done * std::f32::consts::PI).sin().max(0.0)
}

pub(crate) fn paint_band(canvas: &mut Canvas<'_>, palette: &Palette, flow: f32) {
    let (width, height) = (canvas.width().max(1) as f32, canvas.height() as f32);
    let half = height / 2.0;
    for y in 0..canvas.height() {
        let from_middle = ((y as f32 + 0.5 - half) / half).abs();
        let feather = 1.0 - smoothstep(0.25, 1.0, from_middle);
        let line = 1.0 - smoothstep(0.0, 0.08, from_middle);
        for x in 0..canvas.width() {
            let tone = palette.at(x as f32 / width * 0.6 + flow);
            let color = Premul::of(tone.mix(WHITE, 0.5), 0.35 * line).over(Premul::of(tone, BAND_ALPHA * feather));
            canvas.set(x, y, color.to_bgra());
        }
    }
}

pub(crate) fn paint_scan_edge(canvas: &mut Canvas<'_>, origin: Point, rect: Rect, palette: &Palette, scale: f32) {
    let perimeter = 2.0 * (rect.width + rect.height).max(1) as f32;
    let glow = scaled(1.6, scale);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (px, py) = (origin.x as f32 + x as f32 + 0.5, origin.y as f32 + y as f32 + 0.5);
            let outside = (rect.x as f32 - px)
                .max(px - rect.right() as f32)
                .max(rect.y as f32 - py)
                .max(py - rect.bottom() as f32);
            let along = ((px - rect.x as f32) + (py - rect.y as f32)) / perimeter * 2.0;
            let tone = palette.at(along);
            if outside <= 0.0 {
                canvas.set(x, y, 0);
                continue;
            }
            let line = (1.0 - (outside - 0.5).abs()).clamp(0.0, 1.0);
            let halo = 0.5 * (-(outside / glow).powi(2)).exp();
            canvas.set(x, y, Premul::of(tone.mix(WHITE, 0.3), line).over(Premul::of(tone, halo)).to_bgra());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::canvas::Bitmap;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const RECT: Rect = Rect::new(100, 200, 800, 600);

    #[test]
    fn the_band_sweeps_from_top_to_bottom_with_easing() {
        let half = band_height(1.0) / 2;
        assert_eq!(band_top(RECT, Duration::ZERO, 1.0), 200 - half);
        assert_eq!(band_top(RECT, MS(420), 1.0), 800 - half);
        assert_eq!(band_top(RECT, MS(210), 1.0), 500 - half);
        let early = band_top(RECT, MS(40), 1.0) - band_top(RECT, Duration::ZERO, 1.0);
        let middle = band_top(RECT, MS(230), 1.0) - band_top(RECT, MS(190), 1.0);
        assert!(early < middle);
    }

    #[test]
    fn the_edge_glow_fades_in_and_out_with_the_sweep() {
        assert!(crate::overlay::easing::close(edge_alpha(Duration::ZERO, true), 0.0));
        assert!((edge_alpha(MS(210), true) - EDGE_ALPHA).abs() < 1e-3);
        assert!(edge_alpha(MS(420), true) < 1e-3);
        assert!(crate::overlay::easing::close(edge_alpha(MS(100), false), STILL_EDGE_ALPHA));
        assert!(crate::overlay::easing::close(edge_alpha(MS(300), false), 0.0));
    }

    #[test]
    fn the_band_is_faint_and_feathered() {
        let mut bitmap = Bitmap::new(200, band_height(1.0) as usize);
        paint_band(&mut bitmap.canvas().unwrap(), &Palette::aurora(), 0.0);
        let middle = bitmap.pixel(100, bitmap.height / 2) >> 24;
        assert!(middle > 40 && middle < 140, "{middle}");
        assert!(bitmap.pixel(100, 0) >> 24 < 5);
        let quarter = bitmap.pixel(100, bitmap.height / 4) >> 24;
        assert!(quarter > 20 && quarter < 60, "{quarter}");
    }

    #[test]
    fn the_edge_is_a_thin_line_on_the_border() {
        let width = edge_width(1.0);
        let mut bitmap = Bitmap::new(10, width as usize);
        let origin = Point::new(150, RECT.y - width);
        paint_scan_edge(&mut bitmap.canvas().unwrap(), origin, RECT, &Palette::aurora(), 1.0);
        assert!(bitmap.pixel(5, (width - 1) as usize) >> 24 > 200);
        assert!(bitmap.pixel(5, 0) >> 24 < 80);
    }
}
