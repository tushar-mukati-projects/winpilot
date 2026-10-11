use deckhand_core::{Context, Result};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

pub(crate) fn resize_bgra(source: &[u8], from: (u32, u32), to: (u32, u32)) -> Result<Vec<u8>> {
    let source = ImageRef::new(from.0, from.1, source, PixelType::U8x4).context("preparing the resize source")?;
    let mut target = Image::new(to.0, to.1, PixelType::U8x4);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)).use_alpha(false);
    Resizer::new().resize(&source, &mut target, &options).context("resizing the image")?;
    Ok(target.into_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_a_flat_image() {
        let source = [10u8, 20, 30, 255].repeat(8 * 4);
        let resized = resize_bgra(&source, (8, 4), (4, 2)).unwrap();
        assert_eq!(resized.len(), 4 * 2 * 4);
        assert!(resized.as_chunks::<4>().0.iter().all(|pixel| *pixel == [10, 20, 30, 255]));
    }

    #[test]
    fn short_sources_are_rejected() {
        assert!(resize_bgra(&[0u8; 12], (2, 2), (1, 1)).is_err());
    }
}
