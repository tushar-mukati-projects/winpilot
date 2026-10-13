use winpilot_core::{Point, Rect};

use super::{
    canvas::Canvas,
    glow_profile::GlowProfile,
    glow_tints::{GlowTints, Tint},
    perimeter::Perimeter,
};

#[derive(Debug, Clone, Copy, Default)]
struct Axis {
    gap: f32,
    before: bool,
    alphas: (u16, u16),
}

#[derive(Default)]
pub(crate) struct BorderPainter {
    columns: Vec<Axis>,
    rows: Vec<Axis>,
    above: Vec<u32>,
    below: Vec<u32>,
    left: Vec<u32>,
    right: Vec<u32>,
}

struct Span {
    start: usize,
    end: usize,
}

impl BorderPainter {
    pub(crate) fn paint(
        &mut self,
        canvas: &mut Canvas<'_>,
        origin: Point,
        rect: Rect,
        profile: &GlowProfile,
        tints: &GlowTints,
    ) {
        self.measure(canvas, origin, rect, profile, tints);
        let inner = self.inner_columns();
        let mut previous_side: Option<(usize, u32, u32)> = None;
        for (y, row) in self.rows.iter().enumerate() {
            if row.gap > 0.0 {
                let indices = if row.before { &self.above } else { &self.below };
                paint_cap(canvas.row_mut(y), &self.columns, (indices, tints), *row, &inner, profile);
                previous_side = None;
                continue;
            }
            let pair = (self.left.get(y).copied().unwrap_or(0), self.right.get(y).copied().unwrap_or(0));
            match previous_side {
                Some((from, left, right)) if (left, right) == pair => canvas.copy_row(from, y),
                _ => {
                    paint_side(canvas.row_mut(y), &self.columns, (tints.tint(pair.0), tints.tint(pair.1)), &inner);
                    previous_side = Some((y, pair.0, pair.1));
                }
            }
        }
    }

    fn measure(&mut self, canvas: &Canvas<'_>, origin: Point, rect: Rect, profile: &GlowProfile, tints: &GlowTints) {
        let perimeter = Perimeter::of(rect);
        let reach = Reach::of(canvas, origin, rect);
        let columns = (0..canvas.width()).map(|x| origin.x as f32 + x as f32 + 0.5);
        let rows = (0..canvas.height()).map(|y| origin.y as f32 + y as f32 + 0.5);
        let axis = |gap: f32, before: bool| Axis { gap, before, alphas: profile.at(gap) };
        refill(&mut self.columns, columns.clone().map(|x| axis(perimeter.horizontal_gap(x), x < rect.x as f32)));
        refill(&mut self.rows, rows.clone().map(|y| axis(perimeter.vertical_gap(y), y < rect.y as f32)));
        let tint = |wanted: bool, along: f32| if wanted { tints.index_at(along) } else { 0 };
        refill(&mut self.above, columns.clone().map(|x| tint(reach.above, perimeter.along_top(x))));
        refill(&mut self.below, columns.map(|x| tint(reach.below, perimeter.along_bottom(x))));
        refill(&mut self.left, rows.clone().map(|y| tint(reach.left, perimeter.along_left(y))));
        refill(&mut self.right, rows.map(|y| tint(reach.right, perimeter.along_right(y))));
    }

    fn inner_columns(&self) -> Span {
        let width = self.columns.len();
        let start = self.columns.iter().position(|column| column.gap <= 0.0).unwrap_or(width);
        let after = self.columns.get(start..).unwrap_or_default();
        let end = start + after.iter().position(|column| column.gap > 0.0).unwrap_or(after.len());
        Span { start, end }
    }
}

fn refill<T>(target: &mut Vec<T>, values: impl Iterator<Item = T>) {
    target.clear();
    target.extend(values);
}

fn paint_cap(
    line: &mut [u32],
    columns: &[Axis],
    tints: (&[u32], &GlowTints),
    row: Axis,
    inner: &Span,
    profile: &GlowProfile,
) {
    if row.alphas == (0, 0) {
        line.fill(0);
        return;
    }
    let (indices, table) = tints;
    let (core, bloom) = row.alphas;
    let (start, end) = (inner.start.min(line.len()), inner.end.min(line.len()));
    let (head, rest) = line.split_at_mut(start);
    let (middle, tail) = rest.split_at_mut(end - start);
    let slice = |range: std::ops::Range<usize>| indices.get(range).unwrap_or_default();
    paint_corner(head, columns.get(..start).unwrap_or_default(), (slice(0..start), table), row, profile);
    let mut last = (u32::MAX, 0u32);
    for (pixel, index) in middle.iter_mut().zip(slice(start..end)) {
        if *index != last.0 {
            last = (*index, table.tint(*index).pixel(core, bloom));
        }
        *pixel = last.1;
    }
    paint_corner(tail, columns.get(end..).unwrap_or_default(), (slice(end..indices.len()), table), row, profile);
}

fn paint_corner(line: &mut [u32], columns: &[Axis], tints: (&[u32], &GlowTints), row: Axis, profile: &GlowProfile) {
    let (indices, table) = tints;
    for ((pixel, column), index) in line.iter_mut().zip(columns).zip(indices) {
        let (core, bloom) = profile.at(column.gap.hypot(row.gap));
        *pixel = table.tint(*index).pixel(core, bloom);
    }
}

fn paint_side(line: &mut [u32], columns: &[Axis], tints: (Tint, Tint), inner: &Span) {
    let (start, end) = (inner.start.min(line.len()), inner.end.min(line.len()));
    let (head, rest) = line.split_at_mut(start);
    let (middle, tail) = rest.split_at_mut(end - start);
    middle.fill(0);
    for (pixel, column) in
        head.iter_mut().chain(tail.iter_mut()).zip(columns.iter().take(start).chain(columns.iter().skip(end)))
    {
        let tint = if column.before { tints.0 } else { tints.1 };
        *pixel = tint.pixel(column.alphas.0, column.alphas.1);
    }
}

struct Reach {
    above: bool,
    below: bool,
    left: bool,
    right: bool,
}

impl Reach {
    fn of(canvas: &Canvas<'_>, origin: Point, rect: Rect) -> Self {
        let width = i32::try_from(canvas.width()).unwrap_or(i32::MAX);
        let height = i32::try_from(canvas.height()).unwrap_or(i32::MAX);
        Self {
            above: origin.y < rect.y,
            below: origin.y.saturating_add(height) > rect.bottom(),
            left: origin.x < rect.x,
            right: origin.x.saturating_add(width) > rect.right(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::frame::inflate;
    use crate::overlay::{
        border_geometry::BorderMetrics, canvas::Bitmap, edge_strips::border_strips, glow_tints::GlowLook,
        palette::Palette,
    };

    const RECT: Rect = Rect::new(40, 30, 300, 160);

    fn tints_for(rect: Rect, look: &GlowLook) -> GlowTints {
        let mut tints = GlowTints::default();
        tints.refresh(&Perimeter::of(rect), &Palette::aurora(), look);
        tints
    }

    fn render(look: &GlowLook) -> (Bitmap, Rect) {
        let metrics = BorderMetrics::at_scale(1.0);
        let area = inflate(RECT, metrics.band);
        let mut bitmap = Bitmap::new(area.width as usize, area.height as usize);
        let profile = GlowProfile::new(metrics);
        let tints = tints_for(RECT, look);
        BorderPainter::default().paint(
            &mut bitmap.canvas().unwrap(),
            Point::new(area.x, area.y),
            RECT,
            &profile,
            &tints,
        );
        (bitmap, area)
    }

    fn alpha(pixel: u32) -> u32 {
        pixel >> 24
    }

    #[test]
    fn the_inside_of_the_rect_stays_transparent() {
        let (bitmap, area) = render(&GlowLook { flow: 0.3, shimmer: 0.6, ignite: Some(0.5), gain: 1.5 });
        for y in RECT.y..RECT.bottom() {
            for x in RECT.x..RECT.right() {
                assert_eq!(bitmap.pixel((x - area.x) as usize, (y - area.y) as usize), 0, "({x},{y})");
            }
        }
    }

    #[test]
    fn every_pixel_is_premultiplied() {
        let (bitmap, _) = render(&GlowLook { gain: 2.5, ..GlowLook::STILL });
        for pixel in &bitmap.pixels {
            let alpha = alpha(*pixel);
            assert!((0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= alpha), "{pixel:08x}");
        }
    }

    #[test]
    fn a_crisp_line_sits_on_the_edge_with_a_wide_soft_bloom() {
        let (bitmap, area) = render(&GlowLook::STILL);
        let column = (RECT.center().x - area.x) as usize;
        let at = |outside: i32| alpha(bitmap.pixel(column, (RECT.y - area.y - 1 - outside) as usize));
        assert!(at(0) > 200 && at(1) > 200);
        assert!(at(2) < at(1) && at(6) < at(2) && at(14) < at(6) && at(14) > 10);
        assert!(at(26) < 4);
        assert_eq!(bitmap.pixel(0, 0) >> 24, 0);
    }

    #[test]
    fn corners_are_rounded() {
        let (bitmap, area) = render(&GlowLook::STILL);
        let (corner_x, corner_y) = ((RECT.x - area.x) as usize, (RECT.y - area.y) as usize);
        let diagonal = alpha(bitmap.pixel(corner_x - 3, corner_y - 3));
        let straight = alpha(bitmap.pixel(corner_x + 20, corner_y - 3));
        assert!(diagonal < straight && diagonal > 0);
    }

    #[test]
    fn the_colour_changes_around_the_frame() {
        let (bitmap, area) = render(&GlowLook::STILL);
        let row = (RECT.y - area.y - 1) as usize;
        let left = bitmap.pixel((RECT.x + 10 - area.x) as usize, row);
        let right = bitmap.pixel((RECT.right() - 10 - area.x) as usize, row);
        assert_ne!(left & 0x00FF_FFFF, right & 0x00FF_FFFF);
    }

    #[test]
    fn strips_paint_the_same_as_the_whole_frame() {
        let look = GlowLook { flow: 0.4, shimmer: 0.1, ignite: Some(0.7), gain: 1.0 };
        let (whole, area) = render(&look);
        let metrics = BorderMetrics::at_scale(1.0);
        let profile = GlowProfile::new(metrics);
        let tints = tints_for(RECT, &look);
        let mut painter = BorderPainter::default();
        for strip in border_strips(RECT, metrics.band, area).into_iter().flatten() {
            let mut part = Bitmap::new(strip.width as usize, strip.height as usize);
            painter.paint(&mut part.canvas().unwrap(), Point::new(strip.x, strip.y), RECT, &profile, &tints);
            for y in 0..part.height {
                for x in 0..part.width {
                    let (wx, wy) = ((strip.x - area.x) as usize + x, (strip.y - area.y) as usize + y);
                    assert_eq!(part.pixel(x, y), whole.pixel(wx, wy), "({wx},{wy})");
                }
            }
        }
    }

    #[test]
    fn a_full_screen_frame_paints_quickly() {
        let rect = Rect::new(0, 0, 2560, 1600);
        let metrics = BorderMetrics::at_scale(1.0);
        let profile = GlowProfile::new(metrics);
        let strips = border_strips(rect, metrics.band, Rect::new(-100, -100, 4000, 4000));
        let mut painter = BorderPainter::default();
        let mut tints = GlowTints::default();
        let mut bitmaps: Vec<Bitmap> =
            strips.iter().flatten().map(|strip| Bitmap::new(strip.width as usize, strip.height as usize)).collect();
        let started = Instant::now();
        for step in 0..10 {
            let look = GlowLook { flow: step as f32 / 10.0, ..GlowLook::STILL };
            tints.refresh(&Perimeter::of(rect), &Palette::aurora(), &look);
            for (strip, bitmap) in strips.iter().flatten().zip(&mut bitmaps) {
                painter.paint(&mut bitmap.canvas().unwrap(), Point::new(strip.x, strip.y), rect, &profile, &tints);
            }
        }
        let each = started.elapsed() / 10;
        assert!(each.as_millis() < 60, "painting a full-screen frame took {each:?}");
    }
}
