#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

pub(crate) const ACCENT: Rgb = Rgb { red: 0x3B, green: 0x82, blue: 0xF6 };

pub(crate) fn premultiplied(color: Rgb, alpha: u8) -> u32 {
    let scale = |channel: u8| u32::from(channel) * u32::from(alpha) / 255;
    (u32::from(alpha) << 24) | (scale(color.red) << 16) | (scale(color.green) << 8) | scale(color.blue)
}

pub(crate) fn paint_border(pixels: &mut [u32], width: usize, thickness: usize, pixel: u32) {
    if width == 0 {
        return;
    }
    let height = pixels.len() / width;
    for (row_index, row) in pixels.chunks_exact_mut(width).enumerate() {
        let in_band = row_index < thickness || row_index + thickness >= height;
        if in_band {
            row.fill(pixel);
        } else {
            paint_row_edges(row, thickness, pixel);
        }
    }
}

fn paint_row_edges(row: &mut [u32], thickness: usize, pixel: u32) {
    let edge = thickness.min(row.len());
    row[..edge].fill(pixel);
    let right_start = row.len() - edge;
    row[right_start..].fill(pixel);
}

pub(crate) fn paint_disc(pixels: &mut [u32], diameter: usize, color: Rgb, alpha: u8) {
    if diameter == 0 {
        return;
    }
    let radius = diameter as f32 / 2.0;
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let x = (index % diameter) as f32 + 0.5 - radius;
        let y = (index / diameter) as f32 + 0.5 - radius;
        let coverage = (radius - x.hypot(y) + 0.5).clamp(0.0, 1.0);
        *pixel = premultiplied(color, (f32::from(alpha) * coverage).round() as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplication_scales_channels_by_alpha() {
        assert_eq!(premultiplied(ACCENT, 255), 0xFF3B_82F6);
        assert_eq!(premultiplied(ACCENT, 0), 0);
        let half = premultiplied(Rgb { red: 200, green: 100, blue: 50 }, 128);
        assert_eq!(half >> 24, 128);
        assert_eq!((half >> 16) & 0xFF, 200 * 128 / 255);
    }

    #[test]
    fn border_paints_only_the_ring() {
        let (width, height) = (10, 8);
        let mut pixels = vec![0u32; width * height];
        paint_border(&mut pixels, width, 3, 7);
        let at = |x: usize, y: usize| pixels[y * width + x];
        assert_eq!(at(0, 0), 7);
        assert_eq!(at(9, 7), 7);
        assert_eq!(at(2, 4), 7);
        assert_eq!(at(7, 4), 7);
        assert_eq!(at(3, 3), 0);
        assert_eq!(at(6, 4), 0);
        assert_eq!(pixels.iter().filter(|&&p| p == 0).count(), 4 * 2);
    }

    #[test]
    fn thick_border_on_tiny_bitmap_fills_everything_without_panicking() {
        let mut pixels = vec![0u32; 4 * 4];
        paint_border(&mut pixels, 4, 9, 1);
        assert!(pixels.iter().all(|&p| p == 1));
        paint_border(&mut [], 0, 3, 1);
    }

    #[test]
    fn disc_is_opaque_in_the_middle_and_clear_in_corners() {
        let diameter = 28;
        let mut pixels = vec![0u32; diameter * diameter];
        paint_disc(&mut pixels, diameter, ACCENT, 160);
        assert_eq!(pixels[14 * diameter + 14] >> 24, 160);
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[diameter * diameter - 1], 0);
    }
}
