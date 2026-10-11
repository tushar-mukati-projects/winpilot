const BYTES_PER_PIXEL: usize = 4;
const COLOUR_BYTES: usize = 3;

pub fn is_uniform(bgra: &[u8]) -> bool {
    let mut pixels = bgra.as_chunks::<BYTES_PER_PIXEL>().0.iter();
    let Some(first) = pixels.next() else { return true };
    pixels.all(|pixel| pixel[..COLOUR_BYTES] == first[..COLOUR_BYTES])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(colours: &[[u8; 4]]) -> Vec<u8> {
        colours.iter().flatten().copied().collect()
    }

    #[test]
    fn an_all_black_or_single_colour_frame_is_uniform() {
        assert!(is_uniform(&pixels(&[[0, 0, 0, 255]; 16])));
        assert!(is_uniform(&pixels(&[[200, 120, 40, 255]; 9])));
    }

    #[test]
    fn alpha_differences_do_not_count() {
        assert!(is_uniform(&pixels(&[[0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 7]])));
    }

    #[test]
    fn one_different_pixel_breaks_uniformity() {
        let mut frame = pixels(&[[255, 255, 255, 255]; 64]);
        frame[4 * 40 + 1] = 254;
        assert!(!is_uniform(&frame));
    }

    #[test]
    fn empty_frames_count_as_uniform() {
        assert!(is_uniform(&[]));
        assert!(is_uniform(&[1, 2, 3]));
    }
}
