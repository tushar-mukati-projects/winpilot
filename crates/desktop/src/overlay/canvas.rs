use deckhand_core::Rect;

use super::color::Premul;

pub(crate) struct Canvas<'a> {
    pixels: &'a mut [u32],
    stride: usize,
    width: usize,
    height: usize,
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(pixels: &'a mut [u32], stride: usize, width: usize, height: usize) -> Option<Self> {
        let needed = match height {
            0 => 0,
            rows => stride.checked_mul(rows - 1)?.checked_add(width)?,
        };
        (width <= stride && needed <= pixels.len()).then_some(Self { pixels, stride, width, height })
    }

    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    pub(crate) const fn height(&self) -> usize {
        self.height
    }

    pub(crate) fn pixel(&self, x: usize, y: usize) -> u32 {
        self.index(x, y).and_then(|index| self.pixels.get(index)).copied().unwrap_or(0)
    }

    pub(crate) fn set(&mut self, x: usize, y: usize, pixel: u32) {
        if let Some(slot) = self.index(x, y).and_then(|index| self.pixels.get_mut(index)) {
            *slot = pixel;
        }
    }

    pub(crate) fn blend(&mut self, x: usize, y: usize, color: Premul) {
        if color.alpha <= 0.0 {
            return;
        }
        let below = Premul::from_bgra(self.pixel(x, y));
        self.set(x, y, color.over(below).to_bgra());
    }

    pub(crate) fn row_mut(&mut self, y: usize) -> &mut [u32] {
        let start = if y < self.height { y * self.stride } else { self.pixels.len() };
        let end = start.saturating_add(self.width).min(self.pixels.len());
        self.pixels.get_mut(start..end).unwrap_or_default()
    }

    pub(crate) fn copy_row(&mut self, from: usize, to: usize) {
        if from >= self.height || to >= self.height || from == to {
            return;
        }
        let (start, width) = (from * self.stride, self.width);
        let target = to * self.stride;
        if start + width <= self.pixels.len() && target + width <= self.pixels.len() {
            self.pixels.copy_within(start..start + width, target);
        }
    }

    pub(crate) fn fill(&mut self, pixel: u32) {
        for y in 0..self.height {
            self.row_mut(y).fill(pixel);
        }
    }

    pub(crate) fn copy_from(&mut self, source: &Bitmap, region: Rect) {
        let clamp = |value: i32, limit: usize| usize::try_from(value.max(0)).unwrap_or(0).min(limit);
        let columns = self.width.min(source.width);
        let (left, right) = (clamp(region.x, columns), clamp(region.right(), columns));
        let (top, bottom) = (clamp(region.y, self.height), clamp(region.bottom(), self.height.min(source.height)));
        for y in top..bottom {
            let start = y * source.width;
            let (Some(from), Some(to)) =
                (source.pixels.get(start + left..start + right), self.row_mut(y).get_mut(left..right))
            else {
                continue;
            };
            to.copy_from_slice(from);
        }
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width && y < self.height).then(|| y * self.stride + x)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bitmap {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<u32>,
}

impl Bitmap {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![0; width.saturating_mul(height)] }
    }

    pub(crate) fn canvas(&mut self) -> Option<Canvas<'_>> {
        Canvas::new(&mut self.pixels, self.width, self.width, self.height)
    }

    pub(crate) fn pixel(&self, x: usize, y: usize) -> u32 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.pixels.get(y * self.width + x).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::color::Rgb;

    #[test]
    fn canvases_respect_their_stride() {
        let mut pixels = vec![0u32; 10 * 3];
        let mut canvas = Canvas::new(&mut pixels, 10, 4, 3).unwrap();
        canvas.set(3, 2, 7);
        canvas.set(4, 2, 9);
        canvas.set(0, 3, 9);
        assert_eq!(canvas.pixel(3, 2), 7);
        assert_eq!(canvas.pixel(4, 2), 0);
        assert_eq!(pixels[23], 7);
        assert_eq!(pixels.iter().filter(|pixel| **pixel != 0).count(), 1);
    }

    #[test]
    fn regions_copy_from_a_bitmap() {
        let mut source = Bitmap::new(4, 4);
        source.pixels.fill(5);
        let mut pixels = vec![0u32; 6 * 4];
        let mut canvas = Canvas::new(&mut pixels, 6, 4, 4).unwrap();
        canvas.copy_from(&source, Rect::new(1, 1, 2, 9));
        assert_eq!((canvas.pixel(1, 1), canvas.pixel(2, 3), canvas.pixel(0, 1), canvas.pixel(3, 1)), (5, 5, 0, 0));
        canvas.copy_from(&source, Rect::new(-5, -5, 100, 100));
        assert_eq!(pixels.iter().filter(|pixel| **pixel == 5).count(), 16);
    }

    #[test]
    fn rows_copy_within_the_canvas() {
        let mut pixels = vec![0u32; 5 * 3];
        let mut canvas = Canvas::new(&mut pixels, 5, 4, 3).unwrap();
        canvas.set(1, 0, 9);
        canvas.copy_row(0, 2);
        canvas.copy_row(0, 7);
        assert_eq!((canvas.pixel(1, 2), canvas.pixel(1, 1)), (9, 0));
    }

    #[test]
    fn short_buffers_are_refused() {
        let mut pixels = vec![0u32; 10];
        assert!(Canvas::new(&mut pixels, 4, 4, 3).is_none());
        assert!(Canvas::new(&mut pixels, 3, 4, 1).is_none());
        assert!(Canvas::new(&mut pixels, 4, 4, 0).is_some());
    }

    #[test]
    fn blending_composites_over_what_is_there() {
        let mut bitmap = Bitmap::new(2, 2);
        let mut canvas = bitmap.canvas().unwrap();
        canvas.blend(1, 1, Premul::of(Rgb::new(255, 255, 255), 1.0));
        canvas.blend(1, 1, Premul::of(Rgb::new(0, 0, 0), 0.5));
        assert_eq!(canvas.pixel(1, 1), 0xFF80_8080);
        assert_eq!(bitmap.pixel(1, 1), 0xFF80_8080);
    }
}
