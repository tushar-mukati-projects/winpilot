use std::{f32::consts::TAU, time::Duration};

use deckhand_core::{Point, Rect};

use super::{
    canvas::Canvas,
    color::{Premul, WHITE},
    easing::{breath, lerp},
    palette::Palette,
    perimeter::wrapped_distance,
    rounded_box::{RoundedBox, coverage},
    scale::{scaled, scaled_px},
};
use crate::frame::inflate;

const FOCUS_BREATH: Duration = Duration::from_millis(1600);
pub(crate) const FOCUS_FLOW: Duration = Duration::from_millis(4200);
pub(crate) const FOCUS_SHIMMER: Duration = Duration::from_millis(2200);
const LOW_ALPHA: f32 = 0.75;
const HIGH_ALPHA: f32 = 1.0;
pub(crate) const STILL_ALPHA: f32 = 0.85;
const SHINE_STEPS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FocusMetrics {
    gap: i32,
    stroke: f32,
    radius: f32,
    glow: f32,
    pub(crate) inner: i32,
    pub(crate) outer: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FocusLook {
    pub(crate) flow: f32,
    pub(crate) shimmer: f32,
}

impl FocusMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        let stroke = scaled(2.0, scale).round().max(2.0);
        let radius = scaled(6.0, scale);
        let glow = scaled(5.0, scale);
        Self {
            gap: scaled_px(3.0, scale),
            stroke,
            radius,
            glow,
            inner: (stroke + radius * 0.3 + 2.0).ceil() as i32,
            outer: (glow * 2.2).ceil() as i32,
        }
    }

    fn outline(&self, element: Rect) -> Rect {
        inflate(element, self.gap.saturating_add(self.stroke as i32))
    }

    pub(crate) fn band_core(&self, element: Rect) -> Rect {
        inflate(self.outline(element), -self.inner)
    }

    pub(crate) fn band(&self) -> i32 {
        self.inner.saturating_add(self.outer)
    }
}

pub(crate) fn focus_alpha(elapsed: Duration) -> f32 {
    lerp(LOW_ALPHA, HIGH_ALPHA, breath(elapsed, FOCUS_BREATH))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RingPixel {
    x: usize,
    y: usize,
    line: f32,
    glow: f32,
    around: f32,
}

pub(crate) fn focus_ring(origin: Point, size: (usize, usize), element: Rect, metrics: &FocusMetrics) -> Vec<RingPixel> {
    let outline = metrics.outline(element);
    let shape = RoundedBox::new(
        outline.x as f32,
        outline.y as f32,
        outline.width as f32,
        outline.height as f32,
        metrics.radius,
    );
    let center = (outline.x as f32 + outline.width as f32 / 2.0, outline.y as f32 + outline.height as f32 / 2.0);
    let half = ((outline.width as f32 / 2.0).max(1.0), (outline.height as f32 / 2.0).max(1.0));
    let mut ring = Vec::new();
    for y in 0..size.1 {
        for x in 0..size.0 {
            let px = origin.x as f32 + x as f32 + 0.5;
            let py = origin.y as f32 + y as f32 + 0.5;
            let edge = shape.distance(px, py);
            let off_line = (edge + metrics.stroke / 2.0).abs() - metrics.stroke / 2.0;
            let line = coverage(off_line);
            let reach = edge.max(0.0) / metrics.glow.max(1.0);
            let glow = if edge > -metrics.stroke { 0.45 * (-reach * reach).exp() } else { 0.0 };
            if line > 0.0 || glow >= 0.004 {
                let around = ((py - center.1) / half.1).atan2((px - center.0) / half.0) / TAU;
                ring.push(RingPixel { x, y, line, glow, around });
            }
        }
    }
    ring
}

pub(crate) fn shade_ring(canvas: &mut Canvas<'_>, ring: &[RingPixel], palette: &Palette, look: FocusLook) {
    canvas.fill(0);
    let shine: Vec<f32> = (0..SHINE_STEPS)
        .map(|step| {
            let around = step as f32 / SHINE_STEPS as f32;
            0.7 + 0.6 * (-(wrapped_distance(look.shimmer, around, 1.0) / 0.09).powi(2)).exp()
        })
        .collect();
    for pixel in ring {
        let index = ((pixel.around.rem_euclid(1.0) * SHINE_STEPS as f32) as usize).min(SHINE_STEPS - 1);
        let shine = shine.get(index).copied().unwrap_or(1.0);
        let tone = palette.at(pixel.around - look.flow);
        let color =
            Premul::of(tone.mix(WHITE, 0.3), (pixel.line * shine).min(1.0)).over(Premul::of(tone, pixel.glow * shine));
        canvas.set(pixel.x, pixel.y, color.to_bgra());
    }
}

pub(crate) fn paint_focus(
    canvas: &mut Canvas<'_>,
    origin: Point,
    element: Rect,
    metrics: &FocusMetrics,
    style: (&Palette, FocusLook),
) {
    let ring = focus_ring(origin, (canvas.width(), canvas.height()), element, metrics);
    shade_ring(canvas, &ring, style.0, style.1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::canvas::Bitmap;

    const ELEMENT: Rect = Rect::new(50, 40, 200, 30);
    const LOOK: FocusLook = FocusLook { flow: 0.1, shimmer: 0.3 };

    fn render(look: FocusLook) -> (Bitmap, Rect) {
        let metrics = FocusMetrics::at_scale(1.0);
        let area = inflate(metrics.band_core(ELEMENT), metrics.band());
        let mut bitmap = Bitmap::new(area.width as usize, area.height as usize);
        let palette = Palette::aurora();
        paint_focus(&mut bitmap.canvas().unwrap(), Point::new(area.x, area.y), ELEMENT, &metrics, (&palette, look));
        (bitmap, area)
    }

    #[test]
    fn the_outline_surrounds_the_element_without_covering_it() {
        let (bitmap, area) = render(LOOK);
        for y in ELEMENT.y..ELEMENT.bottom() {
            for x in ELEMENT.x..ELEMENT.right() {
                assert_eq!(bitmap.pixel((x - area.x) as usize, (y - area.y) as usize), 0);
            }
        }
        let middle = (ELEMENT.center().x - area.x) as usize;
        let above = (ELEMENT.y - 4 - area.y) as usize;
        assert!(bitmap.pixel(middle, above) >> 24 > 150);
    }

    #[test]
    fn the_band_contains_every_painted_pixel() {
        let metrics = FocusMetrics::at_scale(1.5);
        let core = metrics.band_core(ELEMENT);
        let wide = inflate(core, metrics.band() + 6);
        let mut bitmap = Bitmap::new(wide.width as usize, wide.height as usize);
        let palette = Palette::aurora();
        paint_focus(&mut bitmap.canvas().unwrap(), Point::new(wide.x, wide.y), ELEMENT, &metrics, (&palette, LOOK));
        let band = inflate(core, metrics.band());
        for y in 0..bitmap.height {
            for x in 0..bitmap.width {
                let screen = Point::new(wide.x + x as i32, wide.y + y as i32);
                let inside_band = band.contains(screen) && !core.contains(screen);
                assert!(inside_band || bitmap.pixel(x, y) >> 24 <= 2, "{screen:?}");
            }
        }
    }

    #[test]
    fn the_gradient_flows_and_the_shimmer_travels() {
        let (still, area) = render(LOOK);
        let (moved, _) = render(FocusLook { flow: 0.4, shimmer: 0.8 });
        let at = (ELEMENT.center().x - area.x) as usize;
        let row = (ELEMENT.y - 4 - area.y) as usize;
        assert_ne!(still.pixel(at, row), moved.pixel(at, row));
        assert!(still.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
    }

    #[test]
    fn the_outline_breathes_gently() {
        assert!((focus_alpha(Duration::ZERO) - LOW_ALPHA).abs() < 1e-6);
        assert!((focus_alpha(Duration::from_millis(800)) - HIGH_ALPHA).abs() < 1e-6);
    }
}
