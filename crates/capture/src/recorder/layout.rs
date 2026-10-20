use crate::pixels::{BYTES_PER_PIXEL, Block};

const MAX_VIDEO_HEIGHT: u32 = 2160;
const MIN_SIDE: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub(crate) const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub(crate) const fn pixels(self) -> usize {
        self.width as usize * self.height as usize
    }

    pub(crate) const fn bgra_len(self) -> usize {
        self.pixels() * BYTES_PER_PIXEL
    }
}

pub(crate) fn output_size(source: Size, max_width: u32) -> Size {
    let (width, height) = (f64::from(source.width.max(1)), f64::from(source.height.max(1)));
    let scale = (f64::from(max_width.max(MIN_SIDE)) / width).min(f64::from(MAX_VIDEO_HEIGHT) / height).min(1.0);
    Size::new(even_side(width * scale), even_side(height * scale))
}

fn even_side(length: f64) -> u32 {
    let whole = length.round().max(f64::from(MIN_SIDE)) as u32;
    (whole & !1).max(MIN_SIDE)
}

pub(crate) fn letterbox(source: Size, output: Size) -> Block {
    let (width, height) = (f64::from(source.width.max(1)), f64::from(source.height.max(1)));
    let scale = (f64::from(output.width) / width).min(f64::from(output.height) / height);
    let fitted_width = ((width * scale).round() as u32).clamp(1, output.width.max(1));
    let fitted_height = ((height * scale).round() as u32).clamp(1, output.height.max(1));
    Block {
        x: (output.width.saturating_sub(fitted_width)) / 2,
        y: (output.height.saturating_sub(fitted_height)) / 2,
        width: fitted_width,
        height: fitted_height,
    }
}

impl Block {
    pub(crate) const fn fills(self, output: Size) -> bool {
        self.x == 0 && self.y == 0 && self.width == output.width && self.height == output.height
    }

    pub(crate) const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_sources_scale_down_to_the_max_width_keeping_aspect() {
        assert_eq!(output_size(Size::new(2560, 1600), 1920), Size::new(1920, 1200));
        assert_eq!(output_size(Size::new(3840, 2160), 1920), Size::new(1920, 1080));
        assert_eq!(output_size(Size::new(2560, 1600), 960), Size::new(960, 600));
    }

    #[test]
    fn small_sources_keep_their_size_rounded_down_to_even() {
        assert_eq!(output_size(Size::new(800, 600), 1920), Size::new(800, 600));
        assert_eq!(output_size(Size::new(1001, 667), 1920), Size::new(1000, 666));
        assert_eq!(output_size(Size::new(1, 1), 1920), Size::new(2, 2));
        assert_eq!(output_size(Size::new(0, 0), 1920), Size::new(2, 2));
    }

    #[test]
    fn every_output_side_is_even_and_inside_the_limits() {
        for width in (1..=4000).step_by(37) {
            for height in (1..=3000).step_by(41) {
                let size = output_size(Size::new(width, height), 1920);
                assert!(size.width.is_multiple_of(2) && size.height.is_multiple_of(2), "{width}x{height} -> {size:?}");
                assert!(size.width <= 1920 && size.height <= MAX_VIDEO_HEIGHT, "{width}x{height} -> {size:?}");
                assert!(size.width >= 2 && size.height >= 2);
            }
        }
    }

    #[test]
    fn tall_sources_are_limited_by_height() {
        assert_eq!(output_size(Size::new(1000, 5000), 1920), Size::new(432, 2160));
        assert_eq!(output_size(Size::new(1600, 2560), 3840), Size::new(1350, 2160));
    }

    #[test]
    fn extreme_aspect_ratios_keep_at_least_two_pixels() {
        assert_eq!(output_size(Size::new(10_000, 3), 1920), Size::new(1920, 2));
        assert_eq!(output_size(Size::new(3, 10_000), 1920), Size::new(2, 2160));
    }

    #[test]
    fn letterbox_centres_a_narrower_source() {
        let placement = letterbox(Size::new(800, 800), Size::new(1920, 1200));
        assert_eq!(placement, Block { x: 360, y: 0, width: 1200, height: 1200 });
        assert!(!placement.fills(Size::new(1920, 1200)));
    }

    #[test]
    fn letterbox_centres_a_wider_source() {
        let placement = letterbox(Size::new(3000, 1000), Size::new(1920, 1200));
        assert_eq!(placement, Block { x: 0, y: 280, width: 1920, height: 640 });
    }

    #[test]
    fn letterbox_fills_when_the_aspect_matches() {
        let output = Size::new(1920, 1200);
        assert!(letterbox(Size::new(2560, 1600), output).fills(output));
        assert!(letterbox(Size::new(1920, 1200), output).fills(output));
        assert!(letterbox(Size::new(960, 600), output).fills(output));
    }

    #[test]
    fn letterbox_never_leaves_the_output() {
        for (width, height) in [(1, 1), (1, 5000), (5000, 1), (1919, 1201), (0, 0)] {
            let output = Size::new(1920, 1200);
            let placement = letterbox(Size::new(width, height), output);
            assert!(placement.width >= 1 && placement.height >= 1);
            assert!(placement.x + placement.width <= output.width, "{width}x{height}: {placement:?}");
            assert!(placement.y + placement.height <= output.height, "{width}x{height}: {placement:?}");
        }
    }
}
