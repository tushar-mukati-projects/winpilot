use deckhand_core::{Point, Rect};

use crate::overlay_scale::{scaled, scaled_px};

const SLIDE: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PillMetrics {
    pub(crate) font_px: i32,
    height: i32,
    pad_left: i32,
    pad_right: i32,
    glyph_radius: f32,
    glyph_gap: i32,
    pub(crate) shadow: i32,
    inset: i32,
    margin: i32,
    pub(crate) slide: i32,
}

impl PillMetrics {
    pub(crate) fn at_scale(scale: f32) -> Self {
        Self {
            font_px: scaled_px(13.0, scale),
            height: scaled_px(30.0, scale),
            pad_left: scaled_px(10.0, scale),
            pad_right: scaled_px(14.0, scale),
            glyph_radius: scaled(6.5, scale),
            glyph_gap: scaled_px(7.0, scale),
            shadow: scaled_px(12.0, scale),
            inset: scaled_px(8.0, scale),
            margin: scaled_px(12.0, scale),
            slide: scaled_px(SLIDE, scale),
        }
    }

    fn body_width(&self, text_width: i32) -> i32 {
        let glyph = (self.glyph_radius * 2.0).ceil() as i32;
        self.pad_left
            .saturating_add(glyph)
            .saturating_add(self.glyph_gap)
            .saturating_add(text_width)
            .saturating_add(self.pad_right)
    }

    fn body_height(&self, text_height: i32) -> i32 {
        self.height.max(text_height.saturating_add(self.inset))
    }

    pub(crate) fn fits(&self, room: i32, text_width: i32) -> bool {
        self.body_width(text_width).saturating_add(self.margin.saturating_mul(2)) <= room
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    TopCenter,
    TopRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PillLayout {
    pub(crate) window: Rect,
    pub(crate) body: Rect,
    pub(crate) glyph: (f32, f32),
    pub(crate) glyph_radius: f32,
    pub(crate) text: Point,
}

impl PillLayout {
    pub(crate) fn glyph_region(&self) -> Rect {
        let reach = (self.glyph_radius * 2.2).ceil() as i32;
        let (x, y) = (self.glyph.0.round() as i32, self.glyph.1.round() as i32);
        Rect::new(x - reach, y - reach, reach * 2, reach * 2)
    }

    pub(crate) fn moved_to(self, anchor: Rect, metrics: &PillMetrics, align: Align, screen: Rect) -> Self {
        let size = (self.window.width, self.window.height);
        Self { window: place_window(anchor, metrics, align, size, screen), ..self }
    }

    pub(crate) fn lifted(self, by: i32) -> Self {
        Self {
            window: Rect::new(self.window.x, self.window.y.saturating_sub(by), self.window.width, self.window.height),
            ..self
        }
    }
}

pub(crate) fn layout_pill(
    anchor: Rect,
    metrics: &PillMetrics,
    align: Align,
    text: (i32, i32),
    screen: Rect,
) -> PillLayout {
    let (text_width, text_height) = text;
    let body_width = metrics.body_width(text_width);
    let body_height = metrics.body_height(text_height);
    let body = Rect::new(metrics.shadow, metrics.shadow, body_width, body_height);
    let window_width = body_width.saturating_add(metrics.shadow.saturating_mul(2));
    let window_height = body_height.saturating_add(metrics.shadow.saturating_mul(2));
    let middle = body.y as f32 + body_height as f32 / 2.0;
    let glyph_x = (body.x + metrics.pad_left) as f32 + metrics.glyph_radius;
    let text_x = body.x + metrics.pad_left + (metrics.glyph_radius * 2.0).ceil() as i32 + metrics.glyph_gap;
    let text_y = body.y + (body_height - text_height) / 2;
    PillLayout {
        window: place_window(anchor, metrics, align, (window_width, window_height), screen),
        body,
        glyph: (glyph_x, middle),
        glyph_radius: metrics.glyph_radius,
        text: Point::new(text_x, text_y),
    }
}

pub(crate) fn spring(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    1.0 - (-6.0 * t).exp() * (3.0 * std::f32::consts::PI * t).cos()
}

fn place_window(anchor: Rect, metrics: &PillMetrics, align: Align, size: (i32, i32), screen: Rect) -> Rect {
    let (width, height) = size;
    let left = match align {
        Align::TopCenter => anchor.center().x.saturating_sub(width / 2),
        Align::TopRight => {
            anchor.right().saturating_sub(metrics.inset).saturating_add(metrics.shadow).saturating_sub(width)
        }
    };
    let below_top = anchor.y.saturating_add(metrics.inset).saturating_sub(metrics.shadow);
    let x = clamp_into(left, screen.x, screen.right().saturating_sub(width));
    let y = clamp_into(below_top, screen.y.saturating_sub(metrics.shadow), screen.bottom().saturating_sub(height));
    Rect::new(x, y, width, height)
}

fn clamp_into(value: i32, low: i32, high: i32) -> i32 {
    if high < low { low } else { value.clamp(low, high) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 2560, 1600);

    fn body_on_screen(layout: &PillLayout) -> Rect {
        Rect::new(
            layout.window.x + layout.body.x,
            layout.window.y + layout.body.y,
            layout.body.width,
            layout.body.height,
        )
    }

    #[test]
    fn the_pill_is_centred_eight_pixels_inside_the_top() {
        let metrics = PillMetrics::at_scale(1.0);
        let anchor = Rect::new(200, 100, 1000, 700);
        let layout = layout_pill(anchor, &metrics, Align::TopCenter, (300, 17), SCREEN);
        let body = body_on_screen(&layout);
        assert_eq!(body.y, 108);
        assert!((body.center().x - anchor.center().x).abs() <= 1);
        assert_eq!(body.height, 30);
        assert_eq!(anchor.intersection(body), Some(body));
    }

    #[test]
    fn right_aligned_pills_sit_eight_pixels_inside_the_top_right() {
        let metrics = PillMetrics::at_scale(1.0);
        let anchor = Rect::new(200, 100, 1000, 700);
        let layout = layout_pill(anchor, &metrics, Align::TopRight, (120, 17), SCREEN);
        let body = body_on_screen(&layout);
        assert_eq!((body.right(), body.y), (anchor.right() - 8, 108));
    }

    #[test]
    fn the_text_and_glyph_sit_inside_the_body() {
        let metrics = PillMetrics::at_scale(1.5);
        let layout = layout_pill(Rect::new(0, 0, 1600, 900), &metrics, Align::TopCenter, (420, 26), SCREEN);
        assert!(layout.text.x + 420 <= layout.body.right());
        assert!(layout.text.y >= layout.body.y && layout.text.y + 26 <= layout.body.bottom());
        let glyph = layout.glyph_region();
        assert!(glyph.x >= layout.body.x - 4 && glyph.y >= 0 && glyph.bottom() <= layout.window.height);
    }

    #[test]
    fn narrow_rects_do_not_fit_the_full_label() {
        let metrics = PillMetrics::at_scale(1.0);
        assert!(metrics.fits(500, 380));
        assert!(!metrics.fits(400, 380));
    }

    #[test]
    fn pills_stay_on_the_virtual_screen() {
        let metrics = PillMetrics::at_scale(1.0);
        let layout = layout_pill(Rect::new(-500, -300, 520, 400), &metrics, Align::TopCenter, (200, 17), SCREEN);
        assert!(layout.window.x >= 0);
        assert!(layout.window.y + layout.body.y >= 0);
        let moved = layout.moved_to(Rect::new(2500, 100, 100, 100), &metrics, Align::TopCenter, SCREEN);
        assert_eq!(moved.window.right(), 2560);
        assert_eq!(moved.lifted(12).window.y, moved.window.y - 12);
    }

    #[test]
    fn the_entrance_spring_overshoots_slightly_and_settles() {
        assert!(crate::easing::close(spring(0.0), 0.0));
        let peak = (0..100).map(|step| spring(step as f32 / 100.0)).fold(0.0, f32::max);
        assert!(peak > 1.05 && peak < 1.2, "{peak}");
        assert!((spring(1.0) - 1.0).abs() < 0.01);
    }
}
