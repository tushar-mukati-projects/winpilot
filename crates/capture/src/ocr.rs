use std::borrow::Cow;

use deckhand_core::{Context, Error, Point, Rect, Result};
use windows::Foundation::Rect as WinRect;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrLine as WinOcrLine, OcrResult};
use windows::Security::Cryptography::CryptographicBuffer;
use windows_core::HSTRING;

use crate::com::ComApartment;
use crate::frame::Frame;
use crate::resize::resize_bgra;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrLine {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Scaling {
    origin: Point,
    x: f64,
    y: f64,
}

pub fn ocr(frame: &Frame) -> Result<Vec<OcrLine>> {
    if !frame.is_well_formed() {
        return Err(Error::invalid("frame size does not match its pixel buffer"));
    }
    let _apartment = ComApartment::enter_any()?;
    let engine = create_engine()?;
    let limit = OcrEngine::MaxImageDimension().context("reading the OCR size limit")?;
    let (pixels, width, height) = fit_within(frame, limit)?;
    let scaling = Scaling {
        origin: frame.origin,
        x: f64::from(frame.width) / f64::from(width),
        y: f64::from(frame.height) / f64::from(height),
    };
    let bitmap = software_bitmap(&pixels, width, height)?;
    let result = engine.RecognizeAsync(&bitmap).context("starting OCR")?.join().context("running OCR")?;
    collect_lines(&result, scaling)
}

fn create_engine() -> Result<OcrEngine> {
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(engine);
    }
    Language::CreateLanguage(&HSTRING::from("en-US"))
        .and_then(|language| OcrEngine::TryCreateFromLanguage(&language))
        .map_err(|_| Error::unsupported("no OCR language installed"))
}

fn fit_within(frame: &Frame, limit: u32) -> Result<(Cow<'_, [u8]>, u32, u32)> {
    let limit = limit.max(1);
    if frame.width <= limit && frame.height <= limit {
        return Ok((Cow::Borrowed(&frame.bgra), frame.width, frame.height));
    }
    let scale = (f64::from(frame.width) / f64::from(limit)).max(f64::from(frame.height) / f64::from(limit));
    let shrink = |side: u32| ((f64::from(side) / scale).floor() as u32).clamp(1, limit);
    let (width, height) = (shrink(frame.width), shrink(frame.height));
    let pixels = resize_bgra(&frame.bgra, (frame.width, frame.height), (width, height))?;
    Ok((Cow::Owned(pixels), width, height))
}

fn software_bitmap(bgra: &[u8], width: u32, height: u32) -> Result<SoftwareBitmap> {
    let buffer = CryptographicBuffer::CreateFromByteArray(bgra).context("copying pixels for OCR")?;
    let (width, height) = (width as i32, height as i32);
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width,
        height,
        BitmapAlphaMode::Ignore,
    )
    .context("creating the OCR bitmap")
}

fn collect_lines(result: &OcrResult, scaling: Scaling) -> Result<Vec<OcrLine>> {
    let lines = result.Lines().context("reading OCR lines")?;
    let count = lines.Size().context("counting OCR lines")?;
    let mut collected = Vec::with_capacity(count as usize);
    for index in 0..count {
        let line = lines.GetAt(index).context("reading an OCR line")?;
        collected.push(convert_line(&line, scaling)?);
    }
    Ok(collected)
}

fn convert_line(line: &WinOcrLine, scaling: Scaling) -> Result<OcrLine> {
    let text = line.Text().context("reading OCR line text")?.to_string_lossy();
    let words = line.Words().context("reading OCR words")?;
    let mut boxes = Vec::new();
    for index in 0..words.Size().context("counting OCR words")? {
        boxes.push(words.GetAt(index).context("reading an OCR word")?.BoundingRect().context("reading a word box")?);
    }
    Ok(OcrLine { text, rect: union_to_screen(&boxes, scaling) })
}

fn union_to_screen(boxes: &[WinRect], scaling: Scaling) -> Rect {
    let Some(first) = boxes.first() else {
        return Rect::new(scaling.origin.x, scaling.origin.y, 0, 0);
    };
    let start = (first.X, first.Y, first.X + first.Width, first.Y + first.Height);
    let (left, top, right, bottom) = boxes.iter().fold(start, |(left, top, right, bottom), word| {
        (left.min(word.X), top.min(word.Y), right.max(word.X + word.Width), bottom.max(word.Y + word.Height))
    });
    let to_screen = |value: f32, factor: f64, origin: i32, round: fn(f64) -> f64| {
        f64::from(origin) + round(f64::from(value) * factor)
    };
    let left = to_screen(left, scaling.x, scaling.origin.x, f64::floor);
    let top = to_screen(top, scaling.y, scaling.origin.y, f64::floor);
    let right = to_screen(right, scaling.x, scaling.origin.x, f64::ceil);
    let bottom = to_screen(bottom, scaling.y, scaling.origin.y, f64::ceil);
    Rect::new(left as i32, top as i32, (right - left) as i32, (bottom - top) as i32)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
        DrawTextW, FW_BOLD, FillRect, GetStockObject, HBRUSH, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SetBkMode,
        SetTextColor, TRANSPARENT, WHITE_BRUSH,
    };
    use windows_core::{Owned, w};

    use super::*;
    use crate::gdi::{GdiSurface, Selection};

    fn draw_text_frame(width: u32, height: u32, text_left: i32, origin: Point) -> Frame {
        let surface = GdiSurface::new(width, height).unwrap();
        let dc = surface.dc();
        let everything = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
        let brush = HBRUSH(unsafe { GetStockObject(WHITE_BRUSH) }.0);
        assert_ne!(unsafe { FillRect(dc, &raw const everything, brush) }, 0);
        let font = unsafe {
            CreateFontW(
                96,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            )
        };
        assert!(!font.is_invalid());
        let font: Owned<HFONT> = unsafe { Owned::new(font) };
        let selection = Selection::select(dc, HGDIOBJ::from(*font)).unwrap();
        unsafe { SetBkMode(dc, TRANSPARENT) };
        unsafe { SetTextColor(dc, COLORREF(0)) };
        let mut text: Vec<u16> = "Deckhand 2026".encode_utf16().collect();
        let mut area = RECT { left: text_left, top: 20, right: width as i32, bottom: height as i32 };
        let drawn = unsafe { DrawTextW(dc, &mut text, &raw mut area, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX) };
        assert!(drawn > 0);
        let frame = surface.to_frame(origin).unwrap();
        drop(selection);
        frame
    }

    #[test]
    fn recognises_text_drawn_in_memory() {
        let frame = draw_text_frame(900, 160, 30, Point::new(100, 50));
        let started = Instant::now();
        let lines = ocr(&frame).unwrap();
        eprintln!("timing: OCR of a 900x160 synthetic frame: {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        let line = lines.iter().find(|line| line.text.contains("Deckhand")).unwrap();
        assert!(line.text.contains("2026"));
        assert!(line.rect.x >= 100 + 20 && line.rect.x <= 100 + 60, "{:?}", line.rect);
        assert!(line.rect.y >= 50 && line.rect.bottom() <= 50 + 160, "{:?}", line.rect);
        assert!(line.rect.width > 300);
    }

    #[test]
    fn oversized_frames_are_downscaled_and_mapped_back() {
        let limit = OcrEngine::MaxImageDimension().unwrap();
        if limit > 16_000 {
            return;
        }
        let width = limit * 2;
        let frame = draw_text_frame(width, 300, limit as i32, Point::new(-50, 0));
        let lines = ocr(&frame).unwrap();
        let line = lines.iter().find(|line| line.text.contains("Deckhand")).unwrap();
        let expected_left = limit as i32 - 50;
        assert!((line.rect.x - expected_left).abs() < 60, "{:?} vs {expected_left}", line.rect);
    }

    #[test]
    fn word_boxes_are_unioned_and_scaled() {
        let boxes = [
            WinRect { X: 10.0, Y: 5.0, Width: 20.0, Height: 10.0 },
            WinRect { X: 40.5, Y: 4.0, Width: 10.0, Height: 12.0 },
        ];
        let scaling = Scaling { origin: Point::new(-100, 200), x: 2.0, y: 2.0 };
        assert_eq!(union_to_screen(&boxes, scaling), Rect::new(-80, 208, 81, 24));
        assert_eq!(union_to_screen(&[], scaling), Rect::new(-100, 200, 0, 0));
    }

    #[test]
    fn malformed_frames_are_rejected() {
        let frame = Frame { origin: Point::new(0, 0), width: 5, height: 5, bgra: Vec::new() };
        assert!(ocr(&frame).is_err());
    }
}
