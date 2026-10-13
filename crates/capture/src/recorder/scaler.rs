use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use winpilot_core::{Context, Error, Result};

use super::layout::{Placement, Size, letterbox};
use crate::pixels::BYTES_PER_PIXEL;

const OPAQUE_BLACK: [u8; 4] = [0, 0, 0, 255];

pub(crate) struct Scaler {
    resizer: Resizer,
    options: ResizeOptions,
    fitted: Vec<u8>,
}

impl Scaler {
    pub(crate) fn new() -> Self {
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)).use_alpha(false);
        Self { resizer: Resizer::new(), options, fitted: Vec::new() }
    }

    pub(crate) fn draw(
        &mut self,
        source: &[u8],
        source_size: Size,
        output: &mut [u8],
        output_size: Size,
    ) -> Result<()> {
        if source.len() != source_size.bgra_len() || output.len() != output_size.bgra_len() {
            return Err(Error::internal("frame buffers do not match their sizes"));
        }
        if source_size == output_size {
            output.copy_from_slice(source);
            return Ok(());
        }
        let placement = letterbox(source_size, output_size);
        if placement.fills(output_size) {
            return self.resize(source, source_size, output, output_size);
        }
        let mut fitted = std::mem::take(&mut self.fitted);
        fitted.resize(placement.size().bgra_len(), 0);
        let drawn = self.resize(source, source_size, &mut fitted, placement.size());
        if drawn.is_ok() {
            paste_on_black(&fitted, placement, output, output_size);
        }
        self.fitted = fitted;
        drawn
    }

    fn resize(&mut self, source: &[u8], source_size: Size, target: &mut [u8], target_size: Size) -> Result<()> {
        let from = ImageRef::new(source_size.width, source_size.height, source, PixelType::U8x4)
            .context("preparing the frame for scaling")?;
        let mut to = Image::from_slice_u8(target_size.width, target_size.height, target, PixelType::U8x4)
            .context("preparing the scaled frame")?;
        self.resizer.resize(&from, &mut to, &self.options).context("scaling the frame")
    }
}

fn paste_on_black(fitted: &[u8], placement: Placement, output: &mut [u8], output_size: Size) {
    for pixel in output.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
        *pixel = OPAQUE_BLACK;
    }
    let output_stride = output_size.width as usize * BYTES_PER_PIXEL;
    let row_bytes = placement.width as usize * BYTES_PER_PIXEL;
    let left = placement.x as usize * BYTES_PER_PIXEL;
    for (row, line) in fitted.chunks_exact(row_bytes).enumerate() {
        let start = (placement.y as usize + row) * output_stride + left;
        if let Some(target) = output.get_mut(start..start + row_bytes) {
            target.copy_from_slice(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn flat(size: Size, pixel: [u8; 4]) -> Vec<u8> {
        pixel.repeat(size.pixels())
    }

    fn pixel_at(buffer: &[u8], size: Size, x: u32, y: u32) -> [u8; 4] {
        let start = (y as usize * size.width as usize + x as usize) * 4;
        buffer[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn same_size_frames_are_copied() {
        let size = Size::new(4, 2);
        let source: Vec<u8> = (0..32).collect();
        let mut output = vec![0; 32];
        Scaler::new().draw(&source, size, &mut output, size).unwrap();
        assert_eq!(output, source);
    }

    #[test]
    fn matching_aspect_scales_to_fill() {
        let (from, to) = (Size::new(8, 4), Size::new(4, 2));
        let mut output = vec![0; to.bgra_len()];
        Scaler::new().draw(&flat(from, [10, 20, 30, 255]), from, &mut output, to).unwrap();
        assert!(output.as_chunks::<4>().0.iter().all(|pixel| *pixel == [10, 20, 30, 255]));
    }

    #[test]
    fn other_aspects_are_letterboxed_on_black() {
        let (from, to) = (Size::new(10, 10), Size::new(20, 10));
        let mut output = vec![7; to.bgra_len()];
        Scaler::new().draw(&flat(from, [200, 100, 50, 255]), from, &mut output, to).unwrap();
        assert_eq!(pixel_at(&output, to, 0, 0), OPAQUE_BLACK);
        assert_eq!(pixel_at(&output, to, 4, 9), OPAQUE_BLACK);
        assert_eq!(pixel_at(&output, to, 5, 0), [200, 100, 50, 255]);
        assert_eq!(pixel_at(&output, to, 14, 9), [200, 100, 50, 255]);
        assert_eq!(pixel_at(&output, to, 15, 5), OPAQUE_BLACK);
    }

    #[test]
    fn mismatched_buffers_are_rejected() {
        let size = Size::new(2, 2);
        let mut output = vec![0; 16];
        assert!(Scaler::new().draw(&[0; 15], size, &mut output, size).is_err());
        assert!(Scaler::new().draw(&[0; 16], size, &mut output[..8], size).is_err());
    }

    #[test]
    fn scaling_a_full_screen_frame_is_timed() {
        let (from, to) = (Size::new(2560, 1600), Size::new(1920, 1200));
        let source: Vec<u8> = (0..from.bgra_len()).map(|index| (index * 31 % 253) as u8).collect();
        let mut output = vec![0; to.bgra_len()];
        let mut scaler = Scaler::new();
        scaler.draw(&source, from, &mut output, to).unwrap();
        let started = Instant::now();
        for _ in 0..5 {
            scaler.draw(&source, from, &mut output, to).unwrap();
        }
        let average = started.elapsed().as_secs_f64() * 1000.0 / 5.0;
        eprintln!("timing: scale 2560x1600 to 1920x1200 (bilinear convolution): {average:.2} ms per frame");
    }
}
