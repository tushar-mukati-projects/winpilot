use std::time::Instant;

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::webp::WebPEncoder;

use crate::util::{Frame, median, ms_since};

pub struct Row {
    pub name: String,
    pub ms: f64,
    pub bytes: usize,
}

pub fn target_size(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_edge {
        return (w, h);
    }
    let s = max_edge as f64 / long as f64;
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

pub fn resize_fir(resizer: &mut Resizer, f: &Frame, dw: u32, dh: u32, alg: ResizeAlg) -> Frame {
    let src = Image::from_vec_u8(f.width, f.height, f.bgra.clone(), PixelType::U8x4).unwrap();
    let mut dst = Image::new(dw, dh, PixelType::U8x4);
    let opts = ResizeOptions::new().resize_alg(alg).use_alpha(false);
    resizer.resize(&src, &mut dst, &opts).unwrap();
    Frame { width: dw, height: dh, bgra: dst.into_vec() }
}

pub fn resize_fir_borrowed(resizer: &mut Resizer, f: &mut Frame, dw: u32, dh: u32, alg: ResizeAlg) -> Frame {
    let (w, h) = (f.width, f.height);
    let src = Image::from_slice_u8(w, h, &mut f.bgra, PixelType::U8x4).unwrap();
    let mut dst = Image::new(dw, dh, PixelType::U8x4);
    let opts = ResizeOptions::new().resize_alg(alg).use_alpha(false);
    resizer.resize(&src, &mut dst, &opts).unwrap();
    Frame { width: dw, height: dh, bgra: dst.into_vec() }
}

pub fn bgra_to_rgb(f: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity(f.width as usize * f.height as usize * 3);
    for px in f.bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    out
}

fn time<F: FnMut() -> usize>(runs: usize, mut f: F) -> (f64, usize) {
    let mut v = Vec::new();
    let mut bytes = 0;
    for _ in 0..runs {
        let t = Instant::now();
        bytes = f();
        v.push(ms_since(t));
    }
    (median(&v), bytes)
}

pub fn bench(frame: &Frame, runs: usize) -> (Vec<Row>, Frame) {
    let mut rows = Vec::new();
    let (dw, dh) = target_size(frame.width, frame.height, 1568);
    let mut resizer = Resizer::new();
    let mut work = Frame { width: frame.width, height: frame.height, bgra: frame.bgra.clone() };

    let (ms, _) = time(runs, || resize_fir_borrowed(&mut resizer, &mut work, dw, dh, ResizeAlg::Convolution(FilterType::Bilinear)).bgra.len());
    rows.push(Row { name: format!("resize fast_image_resize bilinear {}x{} -> {dw}x{dh}", frame.width, frame.height), ms, bytes: (dw * dh * 4) as usize });
    let (ms, _) = time(runs, || resize_fir_borrowed(&mut resizer, &mut work, dw, dh, ResizeAlg::Convolution(FilterType::Lanczos3)).bgra.len());
    rows.push(Row { name: "resize fast_image_resize lanczos3".into(), ms, bytes: (dw * dh * 4) as usize });
    let (ms, _) = time(runs, || resize_fir_borrowed(&mut resizer, &mut work, dw, dh, ResizeAlg::Interpolation(FilterType::Bilinear)).bgra.len());
    rows.push(Row { name: "resize fast_image_resize interpolation bilinear".into(), ms, bytes: (dw * dh * 4) as usize });
    let (ms, _) = time(runs.min(3), || {
        let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra.clone()).unwrap();
        image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle).len()
    });
    rows.push(Row { name: "resize image crate triangle (reference)".into(), ms, bytes: (dw * dh * 4) as usize });

    let small = resize_fir(&mut resizer, frame, dw, dh, ResizeAlg::Convolution(FilterType::Bilinear));
    let (ms, _) = time(runs, || bgra_to_rgb(&small).len());
    rows.push(Row { name: "BGRA -> RGB swizzle".into(), ms, bytes: (dw * dh * 3) as usize });
    let rgb = bgra_to_rgb(&small);

    let (ms, bytes) = time(runs, || {
        let mut buf = Vec::with_capacity(512 * 1024);
        JpegEncoder::new_with_quality(&mut buf, 75).write_image(&rgb, dw, dh, image::ExtendedColorType::Rgb8).unwrap();
        buf.len()
    });
    rows.push(Row { name: "JPEG q75 image crate".into(), ms, bytes });
    let (ms, bytes) = time(runs, || {
        let mut buf = Vec::with_capacity(512 * 1024);
        jpeg_encoder::Encoder::new(&mut buf, 75).encode(&small.bgra, dw as u16, dh as u16, jpeg_encoder::ColorType::Bgra).unwrap();
        buf.len()
    });
    rows.push(Row { name: "JPEG q75 jpeg-encoder (simd, BGRA in)".into(), ms, bytes });
    let (ms, bytes) = time(runs, || {
        let mut buf = Vec::with_capacity(1024 * 1024);
        PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, PngFilter::Adaptive).write_image(&rgb, dw, dh, image::ExtendedColorType::Rgb8).unwrap();
        buf.len()
    });
    rows.push(Row { name: "PNG fast image crate".into(), ms, bytes });
    let (ms, bytes) = time(runs, || {
        let mut buf = Vec::with_capacity(1024 * 1024);
        PngEncoder::new_with_quality(&mut buf, CompressionType::Default, PngFilter::Adaptive).write_image(&rgb, dw, dh, image::ExtendedColorType::Rgb8).unwrap();
        buf.len()
    });
    rows.push(Row { name: "PNG default image crate".into(), ms, bytes });
    let (ms, bytes) = time(runs.min(3), || {
        let mut buf = Vec::with_capacity(1024 * 1024);
        WebPEncoder::new_lossless(&mut buf).write_image(&rgb, dw, dh, image::ExtendedColorType::Rgb8).unwrap();
        buf.len()
    });
    rows.push(Row { name: "WebP lossless image crate".into(), ms, bytes });
    let (ms, bytes) = time(runs.min(3), || {
        let mut buf = Vec::with_capacity(1024 * 1024);
        jpeg_encoder::Encoder::new(&mut buf, 75).encode(&frame.bgra, frame.width as u16, frame.height as u16, jpeg_encoder::ColorType::Bgra).unwrap();
        buf.len()
    });
    rows.push(Row { name: format!("JPEG q75 jpeg-encoder full-res {}x{} (reference)", frame.width, frame.height), ms, bytes });
    (rows, small)
}
