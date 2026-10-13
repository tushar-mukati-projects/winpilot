use std::time::Duration;

use deckhand_core::{Point, Rect};

use super::{
    border_geometry::BorderMetrics,
    border_paint::BorderPainter,
    canvas::Bitmap,
    chip_text::ChipText,
    cursor_badge::{BadgeMetrics, layout_badge, paint_badge},
    driving::Driving,
    edge_strips::border_strips,
    focus_outline::{FocusLook, FocusMetrics, paint_focus},
    glow_profile::GlowProfile,
    glow_tints::{GlowLook, GlowTints},
    keycaps::{CapMetrics, layout_caps, paint_caps},
    palette::Palette,
    perimeter::Perimeter,
    pill_kind::PillKind,
    pill_label::PresenceLook,
    pill_layout::{Align, PillMetrics, layout_pill},
    pill_paint::{PillArt, paint_dot, paint_pill, paint_rim, paint_sparkle, pill_rim, rest_rim_on},
    presence::Tone,
    ripple::{Burst, paint_ripple, paint_still_ripple, ripple_reach},
    scan::{band_height, edge_width, paint_band, paint_scan_edge},
    text_mask::TextMask,
    trail::{paint_trail_dot, trail_dots},
    viewfinder::{BracketMetrics, CORNERS, flash_width, paint_bracket, paint_flash_band},
};
use crate::frame::inflate;

fn assert_clean(name: &str, bitmap: &Bitmap) {
    for (index, pixel) in bitmap.pixels.iter().enumerate() {
        let alpha = pixel >> 24;
        let premultiplied = (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= alpha);
        assert!(premultiplied, "{name}: pixel {index} is {pixel:08x}");
    }
    let (width, height) = (bitmap.width, bitmap.height);
    for (x, y) in [(0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1)] {
        assert_eq!(bitmap.pixel(x, y), 0, "{name}: corner ({x},{y}) of {width}x{height} is not clear");
    }
}

fn mask(width: usize, height: usize) -> TextMask {
    TextMask { width, height, coverage: vec![200; width * height] }
}

#[test]
fn border_strips_start_clean() {
    let rect = Rect::new(100, 100, 600, 400);
    let metrics = BorderMetrics::at_scale(1.25);
    let profile = GlowProfile::new(metrics);
    let mut tints = GlowTints::default();
    tints.refresh(&Perimeter::of(rect), &Palette::aurora(), &GlowLook { ignite: Some(0.0), ..GlowLook::STILL });
    for strip in border_strips(rect, metrics.band, Rect::new(0, 0, 2000, 2000)).into_iter().flatten() {
        let mut bitmap = Bitmap::new(strip.width as usize, strip.height as usize);
        BorderPainter::default().paint(
            &mut bitmap.canvas().unwrap(),
            Point::new(strip.x, strip.y),
            rect,
            &profile,
            &tints,
        );
        for pixel in &bitmap.pixels {
            assert!((0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24));
        }
    }
}

#[test]
fn pills_start_clean_with_rounded_corners() {
    let metrics = PillMetrics::at_scale(1.0);
    for (kind, align) in [
        (
            PillKind::Presence(PresenceLook { tone: Tone::Working, driving: Driving::Foreground, room: 800 }),
            Align::TopCenter,
        ),
        (PillKind::Recording { seconds: 3 }, Align::TopRight),
        (PillKind::Activity, Align::BottomCenter),
    ] {
        let layout = layout_pill(Rect::new(0, 0, 800, 600), &metrics, align, (120, 17), Rect::new(0, 0, 1920, 1080));
        let mut bitmap = Bitmap::new(layout.window.width as usize, layout.window.height as usize);
        let lead = mask(120, 17);
        paint_pill(&mut bitmap.canvas().unwrap(), &PillArt { layout, style: kind.style(), lead: &lead, hint: None });
        let mut rim = pill_rim(&layout);
        rest_rim_on(&mut rim, &bitmap);
        let mut canvas = bitmap.canvas().unwrap();
        paint_rim(&mut canvas, &rim, &Palette::aurora(), 0.0);
        paint_sparkle(&mut canvas, &layout, &Palette::aurora(), 0.0, 0.0);
        paint_dot(&mut canvas, &layout, 0.0);
        assert_clean("pill", &bitmap);
        let corner_of_body = bitmap.pixel(layout.body.x as usize, layout.body.y as usize);
        assert!(corner_of_body >> 24 < 120, "the pill's corner should be rounded, not a square box");
    }
}

#[test]
fn ripples_trail_dots_and_badges_start_clean() {
    let palette = Palette::aurora();
    let reach = ripple_reach(1.5);
    let side = (reach * 2) as usize;
    let center = (reach as f32, reach as f32);
    for elapsed in [0, 1, 120] {
        let mut bitmap = Bitmap::new(side, side);
        let burst = Burst { elapsed: Duration::from_millis(elapsed), bursts: 2, palette: &palette, scale: 1.5 };
        paint_ripple(&mut bitmap.canvas().unwrap(), center, &burst);
        assert_clean("ripple", &bitmap);
    }
    let mut quiet = Bitmap::new(side, side);
    paint_still_ripple(&mut quiet.canvas().unwrap(), center, &palette, 1.5);
    assert_clean("still ripple", &quiet);
    for dot in trail_dots(Point::new(0, 0), Point::new(400, 0), 1.5) {
        let diameter = (dot.reach() * 2) as usize;
        let mut bitmap = Bitmap::new(diameter, diameter);
        paint_trail_dot(&mut bitmap.canvas().unwrap(), &dot, palette.at(0.3));
        assert_clean("trail dot", &bitmap);
    }
    let metrics = BadgeMetrics::at_scale(1.0);
    let layout = layout_badge(&metrics, (40, 15));
    let mut badge = Bitmap::new(layout.width as usize, layout.height as usize);
    paint_badge(&mut badge.canvas().unwrap(), &layout, &metrics, &palette, &mask(40, 15));
    assert_clean("badge", &badge);
}

#[test]
fn chips_focus_scan_and_viewfinder_start_clean() {
    let palette = Palette::aurora();
    let metrics = CapMetrics::at_scale(1.0);
    let text = ChipText::keys(vec![vec!["Ctrl".to_string(), "S".to_string()]]);
    let masks = [mask(28, 16), mask(9, 16)];
    let layout = layout_caps(&text.items(), &masks, &mask(8, 16), &metrics);
    let mut chips = Bitmap::new(layout.width as usize, layout.height as usize);
    paint_caps(&mut chips.canvas().unwrap(), &layout, &masks, &mask(8, 16), &palette);
    assert_clean("chips", &chips);

    let element = Rect::new(40, 40, 300, 30);
    let focus = FocusMetrics::at_scale(1.0);
    let area = inflate(focus.band_core(element), focus.band());
    let mut outline = Bitmap::new(area.width as usize, area.height as usize);
    let look = FocusLook { flow: 0.0, shimmer: 0.0 };
    paint_focus(&mut outline.canvas().unwrap(), Point::new(area.x, area.y), element, &focus, (&palette, look));
    assert_clean("focus", &outline);

    let mut band = Bitmap::new(300, band_height(1.0) as usize);
    paint_band(&mut band.canvas().unwrap(), &palette, 0.0);
    for pixel in &band.pixels {
        assert!((0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24));
    }
    let rect = Rect::new(50, 50, 200, 100);
    let width = edge_width(1.0);
    for strip in border_strips(rect, width, Rect::new(0, 0, 500, 500)).into_iter().flatten() {
        let mut edge = Bitmap::new(strip.width as usize, strip.height as usize);
        paint_scan_edge(&mut edge.canvas().unwrap(), Point::new(strip.x, strip.y), rect, &palette, 1.0);
        assert!(edge.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
    }

    let brackets = BracketMetrics::at_scale(1.0);
    for corner in CORNERS {
        let side = brackets.side() as usize;
        let mut bitmap = Bitmap::new(side, side);
        paint_bracket(&mut bitmap.canvas().unwrap(), corner, &brackets, &palette);
        let premultiplied =
            bitmap.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24));
        assert!(premultiplied);
    }
    let flash = flash_width(1.0);
    let mut band = Bitmap::new(200, flash as usize);
    paint_flash_band(&mut band.canvas().unwrap(), Point::new(50, 50), rect, flash);
    assert!(band.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
}
