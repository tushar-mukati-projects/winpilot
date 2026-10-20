use crate::pixels::{BYTES_PER_PIXEL, COLOUR_BYTES};

const LEVELS: usize = 256;
const LOW_PERCENT: usize = 1;
const HIGH_PERCENT: usize = 99;

pub(crate) fn ink_contrast(bgra: &mut [u8]) {
    let (pixels, _) = bgra.as_chunks_mut::<BYTES_PER_PIXEL>();
    let mut histogram = [0usize; LEVELS];
    for pixel in pixels.iter_mut() {
        let gray = pixel[0].min(pixel[1]).min(pixel[2]);
        pixel[..COLOUR_BYTES].fill(gray);
        histogram[usize::from(gray)] += 1;
    }
    let Some((low, high)) = stretch_range(&histogram) else { return };
    let span = u32::from(high - low);
    let stretched: [u8; LEVELS] = std::array::from_fn(|level| {
        let above = (level as u32).saturating_sub(u32::from(low));
        (above * u32::from(u8::MAX) / span).min(u32::from(u8::MAX)) as u8
    });
    for pixel in pixels {
        let level = stretched[usize::from(pixel[0])];
        pixel[..COLOUR_BYTES].fill(level);
    }
}

fn stretch_range(histogram: &[usize; LEVELS]) -> Option<(u8, u8)> {
    let inner = (percentile(histogram, LOW_PERCENT), percentile(histogram, HIGH_PERCENT));
    let darkest = histogram.iter().position(|count| *count > 0)? as u8;
    let lightest = histogram.iter().rposition(|count| *count > 0)? as u8;
    [inner, (darkest, lightest)].into_iter().find(|(low, high)| high > low)
}

fn percentile(histogram: &[usize; LEVELS], percent: usize) -> u8 {
    let rank = histogram.iter().sum::<usize>() * percent / 100;
    let mut seen = 0;
    for (level, count) in histogram.iter().enumerate() {
        seen += count;
        if seen > rank {
            return level as u8;
        }
    }
    u8::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 16;
    const HEIGHT: usize = 8;
    const CYAN: [u8; 4] = [0xD4, 0xBC, 0x00, 0xFF];
    const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
    const HALF_WHITE_ON_CYAN: [u8; 4] = [0xEA, 0xDE, 0x80, 0xFF];
    const BLACK: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];
    const GRAY_INK: [u8; 4] = [0x80, 0x80, 0x80, 0xFF];
    const SPARSE_TILE_PIXELS: usize = 200;

    fn columns(background: [u8; 4], ink: &[(usize, [u8; 4])]) -> Vec<u8> {
        let mut bgra = background.repeat(WIDTH * HEIGHT);
        for row in 0..HEIGHT {
            for (column, color) in ink {
                let at = (row * WIDTH + column) * BYTES_PER_PIXEL;
                bgra[at..at + BYTES_PER_PIXEL].copy_from_slice(color);
            }
        }
        bgra
    }

    fn gray(bgra: &[u8], column: usize) -> i32 {
        let at = (3 * WIDTH + column) * BYTES_PER_PIXEL;
        assert_eq!(bgra[at], bgra[at + 1]);
        assert_eq!(bgra[at], bgra[at + 2]);
        i32::from(bgra[at])
    }

    #[test]
    fn white_ink_on_cyan_stands_out_from_its_background() {
        let mut bgra = columns(CYAN, &[(4, WHITE), (9, HALF_WHITE_ON_CYAN)]);
        ink_contrast(&mut bgra);
        let background = gray(&bgra, 0);
        assert!(gray(&bgra, 4) - background >= 200, "{} vs {background}", gray(&bgra, 4));
        assert!(gray(&bgra, 9) - background >= 100, "{} vs {background}", gray(&bgra, 9));
    }

    #[test]
    fn black_ink_on_white_keeps_its_contrast() {
        let mut bgra = columns(WHITE, &[(4, BLACK)]);
        ink_contrast(&mut bgra);
        assert!(gray(&bgra, 0) - gray(&bgra, 4) >= 250);
    }

    #[test]
    fn sparse_ink_stays_darker_than_its_background() {
        let mut bgra = WHITE.repeat(SPARSE_TILE_PIXELS);
        bgra[..BYTES_PER_PIXEL].copy_from_slice(&GRAY_INK);
        ink_contrast(&mut bgra);
        assert!(bgra[0] < bgra[BYTES_PER_PIXEL], "{} vs {}", bgra[0], bgra[BYTES_PER_PIXEL]);
    }

    #[test]
    fn a_uniform_tile_keeps_its_gray() {
        for colour in [WHITE, GRAY_INK] {
            let mut bgra = colour.repeat(WIDTH * HEIGHT);
            ink_contrast(&mut bgra);
            assert!(bgra.as_chunks::<BYTES_PER_PIXEL>().0.iter().all(|pixel| pixel[..COLOUR_BYTES] == [colour[0]; 3]));
        }
    }
}
