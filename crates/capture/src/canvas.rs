use crate::pixels::BYTES_PER_PIXEL;

pub(crate) type Bgra = [u8; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

impl Bounds {
    pub(crate) const fn sized(left: i64, top: i64, width: i64, height: i64) -> Self {
        Self { left, top, right: left + width, bottom: top + height }
    }
}

pub(crate) struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: i64,
    height: i64,
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(pixels: &'a mut [u8], width: u32, height: u32) -> Self {
        let row_bytes = width as usize * BYTES_PER_PIXEL;
        let rows_present = pixels.len().checked_div(row_bytes).unwrap_or(0);
        let height = (height as usize).min(rows_present);
        Self { pixels, width: i64::from(width), height: height as i64 }
    }

    pub(crate) const fn width(&self) -> i64 {
        self.width
    }

    pub(crate) const fn overlaps(&self, area: Bounds) -> bool {
        area.left < self.width && area.right > 0 && area.top < self.height && area.bottom > 0
    }

    pub(crate) fn fill(&mut self, area: Bounds, color: Bgra) {
        let left = area.left.clamp(0, self.width);
        let right = area.right.clamp(0, self.width);
        let top = area.top.clamp(0, self.height);
        let bottom = area.bottom.clamp(0, self.height);
        if left >= right || top >= bottom {
            return;
        }
        for y in top..bottom {
            let start = ((y * self.width + left) as usize) * BYTES_PER_PIXEL;
            let end = ((y * self.width + right) as usize) * BYTES_PER_PIXEL;
            let Some(row) = self.pixels.get_mut(start..end) else {
                return;
            };
            row.as_chunks_mut::<BYTES_PER_PIXEL>().0.iter_mut().for_each(|pixel| *pixel = color);
        }
    }

    pub(crate) fn stroke(&mut self, area: Bounds, thickness: i64, color: Bgra) {
        let Bounds { left, top, right, bottom } = area;
        self.fill(Bounds { left, top, right, bottom: top + thickness }, color);
        self.fill(Bounds { left, top: bottom - thickness, right, bottom }, color);
        self.fill(Bounds { left, top, right: left + thickness, bottom }, color);
        self.fill(Bounds { left: right - thickness, top, right, bottom }, color);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> Bgra {
        let start = ((y * width + x) * 4) as usize;
        pixels[start..start + 4].try_into().unwrap()
    }

    const RED: Bgra = [0, 0, 255, 255];

    #[test]
    fn fill_is_clipped_to_the_canvas() {
        let mut pixels = vec![0u8; 4 * 4 * 4];
        let mut canvas = Canvas::new(&mut pixels, 4, 4);
        canvas.fill(Bounds::sized(-10, 2, 12, 100), RED);
        canvas.fill(Bounds::sized(i64::MIN / 2, i64::MIN / 2, 5, 5), RED);
        canvas.fill(Bounds::sized(50, 50, 5, 5), RED);
        assert_eq!(pixel_at(&pixels, 4, 0, 1), [0, 0, 0, 0]);
        assert_eq!(pixel_at(&pixels, 4, 1, 2), RED);
        assert_eq!(pixel_at(&pixels, 4, 1, 3), RED);
        assert_eq!(pixel_at(&pixels, 4, 2, 3), [0, 0, 0, 0]);
    }

    #[test]
    fn stroke_draws_only_the_border() {
        let mut pixels = vec![0u8; 8 * 8 * 4];
        let mut canvas = Canvas::new(&mut pixels, 8, 8);
        canvas.stroke(Bounds::sized(1, 1, 6, 6), 2, RED);
        assert_eq!(pixel_at(&pixels, 8, 1, 1), RED);
        assert_eq!(pixel_at(&pixels, 8, 2, 4), RED);
        assert_eq!(pixel_at(&pixels, 8, 6, 6), RED);
        assert_eq!(pixel_at(&pixels, 8, 3, 3), [0, 0, 0, 0]);
        assert_eq!(pixel_at(&pixels, 8, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel_at(&pixels, 8, 7, 7), [0, 0, 0, 0]);
    }

    #[test]
    fn overlap_needs_at_least_one_shared_pixel() {
        let mut pixels = vec![0u8; 4 * 4 * 4];
        let canvas = Canvas::new(&mut pixels, 4, 4);
        assert!(canvas.overlaps(Bounds::sized(3, 3, 5, 5)));
        assert!(canvas.overlaps(Bounds::sized(-2, -2, 3, 3)));
        assert!(!canvas.overlaps(Bounds::sized(4, 0, 2, 2)));
        assert!(!canvas.overlaps(Bounds::sized(-2, 0, 2, 2)));
        assert!(!canvas.overlaps(Bounds::sized(0, -5, 2, 5)));
    }

    #[test]
    fn short_buffers_limit_the_height() {
        let mut pixels = vec![0u8; 4 * 4 * 2];
        let mut canvas = Canvas::new(&mut pixels, 4, 100);
        canvas.fill(Bounds::sized(0, 0, 4, 100), RED);
        assert!(pixels.as_chunks::<4>().0.iter().all(|pixel| *pixel == RED));
    }
}
