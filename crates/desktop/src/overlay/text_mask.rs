#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TextMask {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) coverage: Vec<u8>,
}

impl TextMask {
    pub(crate) fn from_gray_bgra(width: usize, height: usize, pixels: &[u32]) -> Self {
        let coverage = pixels.iter().take(width.saturating_mul(height)).map(|pixel| luminance(*pixel)).collect();
        Self { width, height, coverage }
    }

    pub(crate) fn at(&self, x: usize, y: usize) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.coverage.get(y * self.width + x).copied().unwrap_or(0)
    }
}

fn luminance(pixel: u32) -> u8 {
    let red = (pixel >> 16) & 0xFF;
    let green = (pixel >> 8) & 0xFF;
    let blue = pixel & 0xFF;
    ((red * 54 + green * 183 + blue * 19) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_is_the_luminance_of_white_on_black_text() {
        let mask = TextMask::from_gray_bgra(2, 1, &[0x00FF_FFFF, 0x0080_8080]);
        assert_eq!(mask.at(0, 0), 255);
        assert_eq!(mask.at(1, 0), 128);
        assert_eq!(mask.at(2, 0), 0);
        assert_eq!(mask.at(0, 1), 0);
    }
}
