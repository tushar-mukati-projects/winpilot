use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    time::Duration,
};

use deckhand_core::{Point, Rect};

use crate::{
    border_geometry::BorderMetrics,
    border_paint::BorderPainter,
    canvas::{Bitmap, Canvas},
    cursor_badge::{BadgeMetrics, layout_badge, paint_badge},
    focus_outline::{FocusLook, FocusMetrics, paint_focus},
    frame::inflate,
    glow_profile::GlowProfile,
    glow_tints::{GlowLook, GlowTints},
    keycaps::{CapMetrics, ChipText, Item, layout_caps, paint_caps},
    overlay_color::Premul,
    overlay_palette::{Tone, WHITE},
    palette::Palette,
    perimeter::Perimeter,
    pill_kind::{Glyph, PillKind},
    pill_label::Speaker,
    pill_layout::{PillMetrics, layout_pill, spring},
    pill_paint::{PillArt, paint_dot, paint_pill, paint_rim, paint_sparkle, pill_rim, rest_rim_on},
    ripple::{Burst, paint_ripple, ripple_reach},
    scan::{band_height, band_top, edge_alpha, edge_width, paint_band, paint_scan_edge},
    text_mask::TextMask,
    text_render::{PillFonts, render_text},
    trail::{glide_dots, paint_trail_dot, trail_dots},
    viewfinder::{
        BracketMetrics, CORNERS, FLASH_BORDER_ALPHA, FLASH_TINT_ALPHA, bracket_alpha, bracket_offset, bracket_window,
        flash_level, flash_width, paint_bracket, paint_flash_band,
    },
};

const SPEAKER: Speaker<'static> = Speaker { label: "Claude", hotkey: Some("Ctrl+Alt+Shift+Backspace") };
const GUTTER: usize = 10;

#[derive(Clone, Copy)]
enum Backdrop {
    Dark,
    White,
    Busy,
}

const BACKDROPS: [Backdrop; 3] = [Backdrop::Dark, Backdrop::White, Backdrop::Busy];

impl Backdrop {
    fn at(self, x: usize, y: usize) -> [f32; 3] {
        match self {
            Self::Dark => [0.12, 0.12, 0.14],
            Self::White => [1.0, 1.0, 1.0],
            Self::Busy => busy(x, y),
        }
    }
}

fn busy(x: usize, y: usize) -> [f32; 3] {
    let band = y / 22;
    let toolbar = y < 34;
    if toolbar {
        return [0.93, 0.94, 0.96];
    }
    let column = (x / 160) % 3;
    let hash = (band.wrapping_mul(2_654_435_761) ^ column.wrapping_mul(40_503)) % 97;
    let line_start = 18 + (hash % 9) * 4;
    let line_end = 120 + (hash % 31) * 2;
    let inside_text = (x % 160) > line_start && (x % 160) < line_end && (y % 22) > 8 && (y % 22) < 14;
    let card = column == 2 && band % 6 < 3;
    match (inside_text, card) {
        (true, true) => [0.25, 0.35, 0.65],
        (true, false) => [0.35, 0.36, 0.40],
        (false, true) => [0.86, 0.91, 0.99],
        (false, false) => [0.98, 0.98, 0.985],
    }
}

struct Sheet {
    area: Rect,
    bitmap: Bitmap,
    palette: Palette,
}

impl Sheet {
    fn new(area: Rect) -> Self {
        let palette = Palette::aurora();
        Self { area, bitmap: Bitmap::new(area.width as usize, area.height as usize), palette }
    }

    fn draw(&mut self, at: Rect, alpha: f32, paint: impl FnOnce(&mut Canvas<'_>, Point)) {
        let mut layer = Bitmap::new(at.width.max(0) as usize, at.height.max(0) as usize);
        paint(&mut layer.canvas().unwrap(), Point::new(at.x, at.y));
        let mut canvas = self.bitmap.canvas().unwrap();
        for y in 0..layer.height {
            for x in 0..layer.width {
                let (sx, sy) = (at.x + x as i32 - self.area.x, at.y + y as i32 - self.area.y);
                if sx >= 0 && sy >= 0 {
                    canvas.blend(sx as usize, sy as usize, Premul::from_bgra(layer.pixel(x, y)).scaled(alpha));
                }
            }
        }
    }

    fn border(&mut self, rect: Rect, scale: f32, palette: &Palette, glow: GlowLook) {
        let metrics = BorderMetrics::at_scale(scale);
        let profile = GlowProfile::new(metrics);
        let mut tints = GlowTints::default();
        tints.refresh(&Perimeter::of(rect), palette, &glow);
        self.draw(inflate(rect, metrics.band), 1.0, |canvas, origin| {
            BorderPainter::default().paint(canvas, origin, rect, &profile, &tints);
        });
    }

    fn pill(&mut self, kind: PillKind, anchor: Rect, scale: f32, motion: (f32, f32, f32)) {
        let (pulse, spin, entrance) = motion;
        let metrics = PillMetrics::at_scale(scale);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let label = kind.label(&fonts, &metrics, &SPEAKER).unwrap();
        let lift = (metrics.slide as f32 * (1.0 - spring(entrance))).round() as i32;
        let layout =
            layout_pill(anchor, &metrics, kind.align(), (label.width(), label.height()), self.area).lifted(lift);
        let palette = if kind.alarming() { Palette::crimson() } else { self.palette.clone() };
        let art = PillArt { layout, style: kind.style(), lead: &label.lead, hint: label.hint.as_ref() };
        let mut base = Bitmap::new(layout.window.width as usize, layout.window.height as usize);
        paint_pill(&mut base.canvas().unwrap(), &art);
        let mut rim = pill_rim(&layout);
        rest_rim_on(&mut rim, &base);
        let appear = (entrance / 0.45).min(1.0);
        self.draw(layout.window, appear, |canvas, _| {
            canvas.copy_from(&base, Rect::new(0, 0, layout.window.width, layout.window.height));
            paint_rim(canvas, &rim, &palette, spin);
            match kind.glyph() {
                Glyph::Sparkle => paint_sparkle(canvas, &layout, &palette, spin, pulse),
                Glyph::Dot => paint_dot(canvas, &layout, WHITE, pulse),
            }
        });
    }

    fn ripple(&mut self, center: Point, elapsed: Duration, bursts: u8, scale: f32) {
        let reach = ripple_reach(scale);
        let at = Rect::new(center.x - reach, center.y - reach, reach * 2, reach * 2);
        let palette = self.palette.clone();
        let burst = Burst { elapsed, bursts, palette: &palette, scale };
        self.draw(at, 1.0, |canvas, _| paint_ripple(canvas, (reach as f32, reach as f32), &burst));
    }

    fn dots(&mut self, dots: &[crate::trail::TrailDot], elapsed: Duration) {
        let count = dots.len().max(1) as f32;
        for (index, dot) in dots.iter().enumerate() {
            let reach = dot.reach();
            let at = Rect::new(dot.center.x - reach, dot.center.y - reach, reach * 2, reach * 2);
            let tone = self.palette.at(index as f32 / count * 0.5);
            self.draw(at, dot.alpha(elapsed), |canvas, _| paint_trail_dot(canvas, dot, tone));
        }
    }

    fn badge(&mut self, tip: Point, scale: f32) {
        let metrics = BadgeMetrics::at_scale(scale);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let label = render_text(SPEAKER.label, fonts.lead()).unwrap();
        let layout = layout_badge(&metrics, (label.width as i32, label.height as i32));
        let palette = self.palette.clone();
        self.draw(layout.window_at(tip), 1.0, |canvas, _| paint_badge(canvas, &layout, &metrics, &palette, &label));
    }

    fn chips(&mut self, rect: Rect, text: &ChipText, scale: f32) {
        let metrics = CapMetrics::at_scale(scale);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let items = text.items();
        let masks: Vec<TextMask> = items
            .iter()
            .filter_map(|item| match item {
                Item::Cap(text) => Some(render_text(text, fonts.lead()).unwrap()),
                Item::Plus | Item::Gap => None,
            })
            .collect();
        let plus = render_text("+", fonts.hint()).unwrap();
        let layout = layout_caps(&items, &masks, &plus, &metrics);
        let palette = self.palette.clone();
        self.draw(layout.window_in(rect, &metrics), 1.0, |canvas, _| {
            paint_caps(canvas, &layout, &masks, &plus, &palette);
        });
    }

    fn focus(&mut self, element: Rect, scale: f32, look: FocusLook) {
        let metrics = FocusMetrics::at_scale(scale);
        let at = inflate(metrics.band_core(element), metrics.band());
        let palette = self.palette.clone();
        self.draw(at, 1.0, |canvas, origin| paint_focus(canvas, origin, element, &metrics, (&palette, look)));
    }

    fn viewfinder(&mut self, rect: Rect, elapsed: Duration, scale: f32) {
        let level = flash_level(elapsed, true);
        let tint = Premul::of(WHITE, FLASH_TINT_ALPHA * level).to_bgra();
        self.draw(rect, 1.0, |canvas, _| canvas.fill(tint));
        let width = flash_width(scale);
        self.draw(rect, FLASH_BORDER_ALPHA * level, |canvas, origin| paint_flash_band(canvas, origin, rect, width));
        let metrics = BracketMetrics::at_scale(scale);
        let offset = bracket_offset(elapsed, &metrics, true);
        let alpha = bracket_alpha(elapsed, true);
        let palette = self.palette.clone();
        for corner in CORNERS {
            let at = bracket_window(rect, corner, offset, &metrics);
            self.draw(at, alpha, |canvas, _| paint_bracket(canvas, corner, &metrics, &palette));
        }
    }

    fn scan(&mut self, rect: Rect, elapsed: Duration, scale: f32) {
        let palette = self.palette.clone();
        let width = edge_width(scale);
        let edge = inflate(rect, width);
        self.draw(edge, edge_alpha(elapsed, true), |canvas, origin| {
            paint_scan_edge(canvas, origin, rect, &palette, scale);
        });
        let band = Rect::new(rect.x, band_top(rect, elapsed, scale), rect.width, band_height(scale));
        self.draw(band, 1.0, |canvas, _| paint_band(canvas, &palette, 0.0));
    }

    fn write(&self, folder: &Path, name: &str) {
        let (width, height) = (self.bitmap.width, self.bitmap.height);
        let sheet_width = width * BACKDROPS.len() + GUTTER * (BACKDROPS.len() - 1);
        let mut rgb = vec![255u8; sheet_width * height * 3];
        for (panel, backdrop) in BACKDROPS.iter().enumerate() {
            for y in 0..height {
                for x in 0..width {
                    let color = Premul::from_bgra(self.bitmap.pixel(x, y));
                    let below = backdrop.at(x, y);
                    let offset = (y * sheet_width + panel * (width + GUTTER) + x) * 3;
                    for (channel, part) in [color.red, color.green, color.blue].iter().enumerate() {
                        rgb[offset + channel] =
                            ((part + below[channel] * (1.0 - color.alpha)).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }
        let file = File::create(folder.join(format!("{name}.png"))).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), sheet_width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
}

fn frames_folder() -> Option<PathBuf> {
    let folder = PathBuf::from(std::env::var_os("DECKHAND_OVERLAY_FRAMES")?);
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder)
}

const MS: fn(u64) -> Duration = Duration::from_millis;

fn window_sheet(scale: f32) -> (Sheet, Rect) {
    let rect = Rect::new(50, 50, (640.0 * scale) as i32, (330.0 * scale) as i32);
    (Sheet::new(inflate(rect, 50)), rect)
}

fn flowing(age_ms: u64) -> GlowLook {
    let age = MS(age_ms);
    GlowLook {
        flow: crate::easing::lap_fraction(age, MS(3500)),
        shimmer: crate::easing::lap_fraction(age, MS(2600)),
        ignite: (age_ms < 450).then(|| age_ms as f32 / 450.0),
        gain: 1.0,
    }
}

fn presence(folder: &Path, name: &str, scale: f32, age_ms: u64) {
    let (mut sheet, rect) = window_sheet(scale);
    let palette = sheet.palette.clone();
    sheet.border(rect, scale, &palette, flowing(age_ms));
    let entrance = (age_ms as f32 / 520.0).min(1.0);
    let spin = crate::easing::lap_fraction(MS(age_ms), MS(4000));
    let pulse = crate::easing::breath(MS(age_ms), MS(1600));
    sheet.pill(PillKind::Presence { tone: Tone::Working, room: rect.width }, rect, scale, (pulse, spin, entrance));
    sheet.write(folder, name);
}

fn hero(folder: &Path) {
    let (mut sheet, rect) = window_sheet(1.25);
    let palette = sheet.palette.clone();
    sheet.border(rect, 1.25, &palette, flowing(1800));
    sheet.pill(PillKind::Presence { tone: Tone::Working, room: rect.width }, rect, 1.25, (0.7, 0.45, 1.0));
    let field = Rect::new(rect.x + 120, rect.y + 150, 420, 34);
    sheet.focus(field, 1.25, FocusLook { flow: 0.2, shimmer: 0.15 });
    sheet.dots(
        &glide_dots(Point::new(rect.x + 640, rect.y + 360), Point::new(rect.x + 470, rect.y + 167), MS(160), 1.25),
        MS(150),
    );
    sheet.ripple(Point::new(rect.x + 470, rect.y + 167), MS(110), 1, 1.25);
    sheet.badge(Point::new(rect.x + 470, rect.y + 167), 1.25);
    sheet.chips(rect, &ChipText::Keys(vec![vec!["Ctrl".to_string(), "S".to_string()]]), 1.25);
    sheet.write(folder, "hero-acting");
}

fn small(folder: &Path, name: &str, size: (i32, i32), draw: impl FnOnce(&mut Sheet)) {
    let mut sheet = Sheet::new(Rect::new(0, 0, size.0, size.1));
    draw(&mut sheet);
    sheet.write(folder, name);
}

#[test]
#[ignore = "writes sample overlay frames to DECKHAND_OVERLAY_FRAMES"]
fn write_sample_frames() {
    let Some(folder) = frames_folder() else { return };
    hero(&folder);
    for age in [0, 150, 300, 450] {
        presence(&folder, &format!("border-ignite-{age:03}ms"), 1.0, age);
    }
    presence(&folder, "border-flow-1000ms", 1.0, 1000);
    presence(&folder, "border-flow-2200ms-150dpi", 1.5, 2200);
    let (mut stopped, rect) = window_sheet(1.0);
    let crimson = Palette::crimson();
    stopped.border(rect, 1.0, &crimson, GlowLook { gain: 1.6, ..flowing(2000) });
    stopped.pill(PillKind::Presence { tone: Tone::Stopped, room: rect.width }, rect, 1.0, (1.0, 0.2, 1.0));
    stopped.write(&folder, "stopped");
    for (entrance, name) in [(0.1, "pill-entrance-050ms"), (0.3, "pill-entrance-150ms"), (1.0, "pill-full")] {
        small(&folder, name, (620, 80), |sheet| {
            sheet.pill(
                PillKind::Presence { tone: Tone::Working, room: 600 },
                Rect::new(10, 20, 600, 120),
                1.0,
                (0.8, 0.3, entrance),
            );
        });
    }
    small(&folder, "pill-short", (360, 70), |sheet| {
        sheet.pill(
            PillKind::Presence { tone: Tone::Working, room: 330 },
            Rect::new(15, 10, 330, 120),
            1.0,
            (0.4, 0.6, 1.0),
        );
    });
    small(&folder, "pill-recording", (520, 70), |sheet| {
        sheet.pill(PillKind::Recording { seconds: 12 }, Rect::new(10, 10, 500, 70), 1.0, (0.8, 0.1, 1.0));
    });
    for elapsed in [0, 100, 200, 300, 420] {
        small(&folder, &format!("click-{elapsed:03}ms"), (110, 110), |sheet| {
            sheet.ripple(Point::new(55, 55), MS(elapsed), 1, 1.0);
        });
    }
    small(&folder, "click-double-150ms", (110, 110), |sheet| sheet.ripple(Point::new(55, 55), MS(150), 2, 1.0));
    small(&folder, "trail", (420, 200), |sheet| {
        sheet.dots(&trail_dots(Point::new(40, 160), Point::new(370, 50), 1.0), MS(60));
    });
    small(&folder, "glide-150ms-of-200", (420, 200), |sheet| {
        sheet.dots(&glide_dots(Point::new(40, 160), Point::new(370, 50), MS(200), 1.0), MS(150));
        sheet.badge(Point::new(330, 62), 1.0);
    });
    small(&folder, "cursor-badge", (200, 90), |sheet| sheet.badge(Point::new(40, 30), 1.0));
    small(&folder, "keys-ctrl-shift-s", (420, 110), |sheet| {
        let chords = vec![vec!["Ctrl".to_string(), "Shift".to_string(), "S".to_string()], vec!["Enter".to_string()]];
        sheet.chips(Rect::new(0, 0, 420, 110), &ChipText::Keys(chords), 1.0);
    });
    small(&folder, "keys-typing", (320, 100), |sheet| {
        sheet.chips(Rect::new(0, 0, 320, 100), &ChipText::Typing(42), 1.0);
    });
    small(&folder, "focus-field", (380, 100), |sheet| {
        sheet.focus(Rect::new(30, 34, 320, 32), 1.0, FocusLook { flow: 0.1, shimmer: 0.3 });
    });
    for elapsed in [0, 100, 160, 260, 400] {
        small(&folder, &format!("screenshot-{elapsed:03}ms"), (440, 280), |sheet| {
            sheet.viewfinder(Rect::new(70, 70, 300, 140), MS(elapsed), 1.0);
        });
    }
    for elapsed in [60, 140, 210, 330] {
        small(&folder, &format!("scan-{elapsed:03}ms"), (440, 300), |sheet| {
            sheet.scan(Rect::new(40, 40, 360, 220), MS(elapsed), 1.0);
        });
    }
}
