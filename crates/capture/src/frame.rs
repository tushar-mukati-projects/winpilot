use deckhand_core::{Error, Point, Rect, Result};

use crate::pixels::{BYTES_PER_PIXEL, Block, Pixels, PixelsMut, copy_block};

const COLOUR_BYTES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSource {
    Screen,
    Window,
    ScreenFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub origin: Point,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub source: FrameSource,
}

impl Frame {
    pub fn rect(&self) -> Rect {
        Rect::new(self.origin.x, self.origin.y, self.width as i32, self.height as i32)
    }

    pub fn crop(&self, rect: Rect) -> Option<Self> {
        if !self.is_well_formed() {
            return None;
        }
        let overlap = overlap(self.rect(), rect)?;
        let mut cropped = Self::black(Point::new(overlap.x, overlap.y), overlap.width as u32, overlap.height as u32);
        cropped.source = self.source;
        cropped.paste(self).ok()?;
        Some(cropped)
    }

    pub fn is_uniform(&self) -> bool {
        let mut pixels = self.bgra.as_chunks::<BYTES_PER_PIXEL>().0.iter();
        let Some(first) = pixels.next() else { return true };
        pixels.all(|pixel| pixel[..COLOUR_BYTES] == first[..COLOUR_BYTES])
    }

    pub(crate) fn is_all_black(&self) -> bool {
        self.is_uniform() && self.bgra.get(..COLOUR_BYTES).is_none_or(|first| first == [0, 0, 0])
    }

    pub(crate) fn black(origin: Point, width: u32, height: u32) -> Self {
        let length = width as usize * height as usize * BYTES_PER_PIXEL;
        Self { origin, width, height, bgra: vec![0; length], source: FrameSource::Screen }
    }

    pub(crate) fn is_well_formed(&self) -> bool {
        let expected = u64::from(self.width) * u64::from(self.height) * BYTES_PER_PIXEL as u64;
        self.width > 0 && self.height > 0 && i32::try_from(self.width).is_ok() && self.bgra.len() as u64 == expected
    }

    pub(crate) fn stride(&self) -> usize {
        self.width as usize * BYTES_PER_PIXEL
    }

    pub(crate) fn pixels(&self) -> Pixels<'_> {
        Pixels { bytes: &self.bgra, stride: self.stride() }
    }

    pub(crate) fn pixels_mut(&mut self) -> PixelsMut<'_> {
        let stride = self.stride();
        PixelsMut { bytes: &mut self.bgra, stride }
    }

    pub(crate) fn offset_of(&self, point: Point) -> (usize, usize) {
        let x = i64::from(point.x) - i64::from(self.origin.x);
        let y = i64::from(point.y) - i64::from(self.origin.y);
        (x.max(0) as usize, y.max(0) as usize)
    }

    fn paste(&mut self, piece: &Self) -> Result<()> {
        let overlap =
            overlap(self.rect(), piece.rect()).ok_or_else(|| Error::internal("piece is outside the frame"))?;
        let (x, y) = piece.offset_of(Point::new(overlap.x, overlap.y));
        let block = Block { x, y, width: overlap.width as usize, height: overlap.height as usize };
        let to = self.offset_of(Point::new(overlap.x, overlap.y));
        copy_block(&piece.pixels(), block, &mut self.pixels_mut(), to)
    }
}

pub(crate) fn overlap(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    let width = right - i64::from(left);
    let height = bottom - i64::from(top);
    (width > 0 && height > 0).then(|| Rect::new(left, top, width as i32, height as i32))
}

#[cfg(test)]
mod tests {
    fn painted(colours: &[[u8; 4]]) -> Frame {
        let bgra: Vec<u8> = colours.iter().flatten().copied().collect();
        Frame { origin: Point::new(0, 0), width: colours.len() as u32, height: 1, bgra, source: FrameSource::Window }
    }

    #[test]
    fn uniform_frames_ignore_alpha_and_one_pixel_breaks_them() {
        assert!(painted(&[[200, 120, 40, 255]; 9]).is_uniform());
        assert!(painted(&[[0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 7]]).is_uniform());
        let mut white = painted(&[[255, 255, 255, 255]; 64]);
        white.bgra[4 * 40 + 1] = 254;
        assert!(!white.is_uniform());
        assert!(painted(&[]).is_uniform());
    }

    #[test]
    fn all_black_means_uniform_and_black() {
        let mut frame = Frame::black(Point::new(0, 0), 4, 4);
        frame.bgra.iter_mut().skip(3).step_by(4).for_each(|alpha| *alpha = 255);
        assert!(frame.is_all_black());
        frame.bgra[20] = 1;
        assert!(!frame.is_all_black());
        assert!(!painted(&[[9, 9, 9, 255]; 4]).is_all_black());
    }

    use super::*;

    fn gradient(origin: Point, width: u32, height: u32) -> Frame {
        let mut frame = Frame::black(origin, width, height);
        for (index, pixel) in frame.bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            *pixel = [x as u8, y as u8, 7, 255];
        }
        frame
    }

    fn pixel(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * frame.width + x) * 4) as usize;
        frame.bgra[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn crop_inside_copies_the_region() {
        let frame = gradient(Point::new(100, 50), 40, 30);
        let cropped = frame.crop(Rect::new(110, 60, 5, 4)).unwrap();
        assert_eq!(cropped.rect(), Rect::new(110, 60, 5, 4));
        assert_eq!(pixel(&cropped, 0, 0), [10, 10, 7, 255]);
        assert_eq!(pixel(&cropped, 4, 3), [14, 13, 7, 255]);
    }

    #[test]
    fn crop_clips_to_the_frame() {
        let frame = gradient(Point::new(-20, -10), 40, 30);
        let cropped = frame.crop(Rect::new(10, 15, 100, 100)).unwrap();
        assert_eq!(cropped.rect(), Rect::new(10, 15, 10, 5));
        assert_eq!(pixel(&cropped, 0, 0), [30, 25, 7, 255]);
    }

    #[test]
    fn crop_keeps_the_source() {
        let mut frame = gradient(Point::new(0, 0), 10, 10);
        frame.source = FrameSource::Window;
        assert_eq!(frame.crop(Rect::new(2, 2, 3, 3)).unwrap().source, FrameSource::Window);
    }

    #[test]
    fn crop_without_overlap_is_none() {
        let frame = gradient(Point::new(0, 0), 10, 10);
        assert_eq!(frame.crop(Rect::new(10, 0, 5, 5)), None);
        assert_eq!(frame.crop(Rect::new(0, 0, 0, 5)), None);
        assert_eq!(frame.crop(Rect::new(3, 3, -2, 5)), None);
    }

    #[test]
    fn crop_survives_extreme_coordinates() {
        let frame = gradient(Point::new(i32::MAX - 5, i32::MIN), 10, 10);
        let cropped = frame.crop(Rect::new(i32::MAX - 2, i32::MIN, i32::MAX, i32::MAX)).unwrap();
        assert_eq!(cropped.width, 7);
        assert_eq!(cropped.height, 10);
        assert_eq!(frame.crop(Rect::new(i32::MIN, i32::MIN, i32::MAX, 1)), None);
    }

    #[test]
    fn crop_of_a_malformed_frame_is_none() {
        let mut frame = Frame::black(Point::new(0, 0), 10, 10);
        frame.bgra.truncate(12);
        assert_eq!(frame.crop(Rect::new(0, 0, 5, 5)), None);
    }
}
