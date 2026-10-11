use std::borrow::Cow;

use deckhand_core::{Context, Error, Result, protocol::ImageFormat};

use crate::canvas::Canvas;
use crate::frame::Frame;
use crate::marks::{Mark, draw_marks};
use crate::resize::resize_bgra;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub max_width: u32,
    pub max_height: u32,
    pub format: ImageFormat,
    pub quality: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

pub(crate) fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> Fit {
    let scale = (f64::from(width) / f64::from(max_width)).max(f64::from(height) / f64::from(max_height)).max(1.0);
    let shrink = |side: u32| ((f64::from(side) / scale).round() as u32).max(1);
    Fit { width: shrink(width), height: shrink(height), scale }
}

pub fn encode(frame: &Frame, options: &EncodeOptions, marks: &[Mark]) -> Result<Encoded> {
    validate(frame, options)?;
    let fit = fit(frame.width, frame.height, options.max_width, options.max_height);
    let mut pixels = scaled_pixels(frame, fit)?;
    if !marks.is_empty() {
        draw_marks(&mut Canvas::new(pixels.to_mut(), fit.width, fit.height), marks, frame.origin, fit.scale);
    }
    let (bytes, mime) = match options.format {
        ImageFormat::Jpeg => (encode_jpeg(&pixels, fit.width, fit.height, options.quality)?, "image/jpeg"),
        ImageFormat::Png => (encode_png(&pixels, fit.width, fit.height)?, "image/png"),
    };
    Ok(Encoded { bytes, mime, width: fit.width, height: fit.height, scale: fit.scale })
}

fn validate(frame: &Frame, options: &EncodeOptions) -> Result<()> {
    if !frame.is_well_formed() {
        return Err(Error::invalid("frame size does not match its pixel buffer"));
    }
    if options.max_width == 0 || options.max_height == 0 {
        return Err(Error::invalid("maximum image width and height must be positive"));
    }
    Ok(())
}

fn scaled_pixels(frame: &Frame, fit: Fit) -> Result<Cow<'_, [u8]>> {
    if fit.width == frame.width && fit.height == frame.height {
        return Ok(Cow::Borrowed(&frame.bgra));
    }
    resize_bgra(&frame.bgra, (frame.width, frame.height), (fit.width, fit.height)).map(Cow::Owned)
}

fn encode_jpeg(bgra: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    let too_large = || Error::invalid("JPEG images are limited to 65535 px per side");
    let width = u16::try_from(width).map_err(|_| too_large())?;
    let height = u16::try_from(height).map_err(|_| too_large())?;
    let mut bytes = Vec::with_capacity(256 * 1024);
    jpeg_encoder::Encoder::new(&mut bytes, quality.clamp(1, 100))
        .encode(bgra, width, height, jpeg_encoder::ColorType::Bgra)
        .context("encoding JPEG")?;
    Ok(bytes)
}

fn encode_png(bgra: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let rgb = bgra_to_rgb(bgra);
    let mut bytes = Vec::with_capacity(512 * 1024);
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().context("writing the PNG header")?;
    writer.write_image_data(&rgb).context("writing PNG pixels")?;
    writer.finish().context("finishing the PNG")?;
    Ok(bytes)
}

fn bgra_to_rgb(bgra: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(bgra.len() / 4 * 3);
    for pixel in bgra.as_chunks::<4>().0 {
        rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    rgb
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use deckhand_core::{ErrorCode, Point, Rect};

    use super::*;

    fn options(max_width: u32, max_height: u32, format: ImageFormat) -> EncodeOptions {
        EncodeOptions { max_width, max_height, format, quality: 75 }
    }

    fn checker(width: u32, height: u32) -> Frame {
        let mut frame = Frame::black(Point::new(0, 0), width, height);
        for (index, pixel) in frame.bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let shade = if (x / 8 + y / 8) % 2 == 0 { 230 } else { 20 };
            *pixel = [shade, (x % 256) as u8, (y % 256) as u8, 255];
        }
        frame
    }

    #[test]
    fn fit_shrinks_to_the_tighter_limit() {
        assert_eq!(fit(2560, 1600, 1280, 1280), Fit { width: 1280, height: 800, scale: 2.0 });
        assert_eq!(fit(1000, 4000, 1280, 1280), Fit { width: 320, height: 1280, scale: 3.125 });
        assert_eq!(fit(3840, 2160, 1568, 1568), Fit { width: 1568, height: 882, scale: 3840.0 / 1568.0 });
    }

    #[test]
    fn fit_never_upscales() {
        assert_eq!(fit(800, 600, 1280, 1280), Fit { width: 800, height: 600, scale: 1.0 });
        assert_eq!(fit(1280, 800, 1280, 800), Fit { width: 1280, height: 800, scale: 1.0 });
    }

    #[test]
    fn fit_keeps_every_side_at_least_one_pixel() {
        let thin = fit(10_000, 1, 100, 100);
        assert_eq!((thin.width, thin.height), (100, 1));
        let odd = fit(1001, 3, 500, 500);
        assert_eq!((odd.width, odd.height), (500, 1));
        assert!((odd.scale - 2.002).abs() < 1e-9);
    }

    #[test]
    fn jpeg_has_markers_and_fitted_size() {
        let encoded = encode(&checker(640, 400), &options(320, 320, ImageFormat::Jpeg), &[]).unwrap();
        assert_eq!((encoded.width, encoded.height, encoded.scale), (320, 200, 2.0));
        assert_eq!(encoded.mime, "image/jpeg");
        assert_eq!(&encoded.bytes[..3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(&encoded.bytes[encoded.bytes.len() - 2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn png_has_signature_and_header_size() {
        let encoded = encode(&checker(300, 500), &options(100, 100, ImageFormat::Png), &[]).unwrap();
        assert_eq!((encoded.width, encoded.height, encoded.scale), (60, 100, 5.0));
        assert_eq!(encoded.mime, "image/png");
        assert_eq!(&encoded.bytes[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&encoded.bytes[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(encoded.bytes[16..20].try_into().unwrap()), 60);
        assert_eq!(u32::from_be_bytes(encoded.bytes[20..24].try_into().unwrap()), 100);
    }

    #[test]
    fn marks_are_drawn_without_changing_the_size() {
        let frame = checker(200, 100);
        let marks = [Mark { rect: Rect::new(20, 20, 50, 30), label: "e7".to_string() }];
        let plain = encode(&frame, &options(200, 200, ImageFormat::Png), &[]).unwrap();
        let marked = encode(&frame, &options(200, 200, ImageFormat::Png), &marks).unwrap();
        assert_eq!((marked.width, marked.height), (200, 100));
        assert_ne!(plain.bytes, marked.bytes);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let frame = checker(10, 10);
        let zero = encode(&frame, &options(0, 100, ImageFormat::Jpeg), &[]).unwrap_err();
        assert_eq!(zero.code, ErrorCode::InvalidArgument);
        let broken = Frame { origin: Point::new(0, 0), width: 10, height: 10, bgra: vec![0; 10] };
        let malformed = encode(&broken, &options(100, 100, ImageFormat::Jpeg), &[]).unwrap_err();
        assert_eq!(malformed.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn full_screen_jpeg_encode_timing() {
        let frame = checker(2560, 1600);
        let jpeg = options(1280, 1280, ImageFormat::Jpeg);
        encode(&frame, &jpeg, &[]).unwrap();
        let mut best = f64::MAX;
        for _ in 0..5 {
            let started = Instant::now();
            let encoded = encode(&frame, &jpeg, &[]).unwrap();
            best = best.min(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!((encoded.width, encoded.height), (1280, 800));
        }
        eprintln!("timing: encode 2560x1600 -> 1280x800 JPEG q75 (synthetic frame): best {best:.2} ms");
    }
}
