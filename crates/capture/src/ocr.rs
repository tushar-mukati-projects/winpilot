use std::sync::mpsc;
use std::time::Duration;

use deckhand_core::{
    Context, Error, Point, Rect, Result,
    ocr::{OcrLine, OcrWord},
};
use windows::Foundation::Rect as WinRect;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrLine as WinOcrLine, OcrResult};
use windows::Security::Cryptography::CryptographicBuffer;
use windows_future::{AsyncOperationCompletedHandler, IAsyncOperation};

use crate::com::ComApartment;
use crate::frame::Frame;
use crate::pixels::BYTES_PER_PIXEL;
use crate::resize::{Fit, fit, fitted_pixels, resize_bgra};

const RECOGNITION_TIMEOUT: Duration = Duration::from_secs(10);
const MAGNIFICATION: u32 = 2;
const TILE_OVERLAP: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Scaling {
    origin: Point,
    x: f64,
    y: f64,
}

impl Scaling {
    fn between(frame: &Frame, fitted: Fit) -> Self {
        Self {
            origin: frame.origin,
            x: f64::from(frame.width) / f64::from(fitted.width),
            y: f64::from(frame.height) / f64::from(fitted.height),
        }
    }
}

pub fn ocr(frame: &Frame) -> Result<Vec<OcrLine>> {
    if !frame.is_well_formed() {
        return Err(Error::invalid("frame size does not match its pixel buffer"));
    }
    let _apartment = ComApartment::enter_any()?;
    let engine = create_engine()?;
    let limit = OcrEngine::MaxImageDimension().context("reading the OCR size limit")?;
    let fitted = fit(frame.width, frame.height, limit, limit);
    let pixels = fitted_pixels(frame, fitted)?;
    let bitmap = software_bitmap(&pixels, fitted.width, fitted.height)?;
    let operation = engine.RecognizeAsync(&bitmap).context("starting OCR")?;
    let result = finish_within(&operation, RECOGNITION_TIMEOUT)?;
    collect_lines(&result, Scaling::between(frame, fitted))
}

pub fn ocr_magnified(frame: &Frame) -> Result<Vec<OcrLine>> {
    if !frame.is_well_formed() {
        return Err(Error::invalid("frame size does not match its pixel buffer"));
    }
    let _apartment = ComApartment::enter_any()?;
    let engine = create_engine()?;
    let limit = OcrEngine::MaxImageDimension().context("reading the OCR size limit")?;
    let side = (limit / MAGNIFICATION).max(TILE_OVERLAP * 2);
    let mut lines: Vec<OcrLine> = Vec::new();
    for tile in tiles(frame.width, frame.height, side, TILE_OVERLAP) {
        for line in magnified_tile_lines(&engine, frame, tile)? {
            if !lines.iter().any(|seen| same_line(seen, &line)) {
                lines.push(line);
            }
        }
    }
    Ok(lines)
}

fn magnified_tile_lines(engine: &OcrEngine, frame: &Frame, tile: Tile) -> Result<Vec<OcrLine>> {
    let (width, height) = (tile.width * MAGNIFICATION, tile.height * MAGNIFICATION);
    let pixels = resize_bgra(&cropped(frame, tile), (tile.width, tile.height), (width, height))?;
    let bitmap = software_bitmap(&pixels, width, height)?;
    let operation = engine.RecognizeAsync(&bitmap).context("starting OCR")?;
    let result = finish_within(&operation, RECOGNITION_TIMEOUT)?;
    let origin = Point::new(frame.origin.x + tile.x as i32, frame.origin.y + tile.y as i32);
    let shrink = 1.0 / f64::from(MAGNIFICATION);
    collect_lines(&result, Scaling { origin, x: shrink, y: shrink })
}

fn tiles(width: u32, height: u32, side: u32, overlap: u32) -> Vec<Tile> {
    let columns = tile_starts(width, side, overlap);
    tile_starts(height, side, overlap)
        .into_iter()
        .flat_map(|y| {
            columns.iter().map(move |&x| Tile { x, y, width: side.min(width - x), height: side.min(height - y) })
        })
        .collect()
}

fn tile_starts(length: u32, side: u32, overlap: u32) -> Vec<u32> {
    let step = side.saturating_sub(overlap).max(1);
    let mut starts = vec![0];
    let mut start = 0;
    while start + side < length {
        start += step;
        starts.push(start);
    }
    starts
}

fn cropped(frame: &Frame, tile: Tile) -> Vec<u8> {
    let stride = frame.width as usize * BYTES_PER_PIXEL;
    let row_bytes = tile.width as usize * BYTES_PER_PIXEL;
    let left = tile.x as usize * BYTES_PER_PIXEL;
    let mut pixels = Vec::with_capacity(row_bytes * tile.height as usize);
    for row in tile.y..tile.y + tile.height {
        let start = row as usize * stride + left;
        if let Some(bytes) = frame.bgra.get(start..start + row_bytes) {
            pixels.extend_from_slice(bytes);
        }
    }
    pixels
}

fn same_line(seen: &OcrLine, line: &OcrLine) -> bool {
    seen.text.trim().eq_ignore_ascii_case(line.text.trim()) && seen.rect.intersection(line.rect).is_some()
}

fn create_engine() -> Result<OcrEngine> {
    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(engine);
    }
    first_available_language()
        .and_then(|language| OcrEngine::TryCreateFromLanguage(&language).ok())
        .ok_or_else(|| Error::unsupported("no OCR language installed"))
}

fn first_available_language() -> Option<Language> {
    let languages = OcrEngine::AvailableRecognizerLanguages().ok()?;
    if languages.Size().ok()? == 0 {
        return None;
    }
    languages.GetAt(0).ok()
}

fn finish_within(operation: &IAsyncOperation<OcrResult>, timeout: Duration) -> Result<OcrResult> {
    let (finished, completion) = mpsc::channel();
    let handler = AsyncOperationCompletedHandler::new(move |_, status| {
        let _ = finished.send(status);
        Ok(())
    });
    operation.SetCompleted(&handler).context("waiting for OCR")?;
    if completion.recv_timeout(timeout).is_err() {
        return Err(cancelled_after(operation, timeout));
    }
    operation.GetResults().context("running OCR")
}

fn cancelled_after(operation: &IAsyncOperation<OcrResult>, timeout: Duration) -> Error {
    let outcome = match operation.Cancel() {
        Ok(()) => "it was cancelled",
        Err(_) => "cancelling it failed",
    };
    Error::timeout(format!("OCR did not finish within {} s; {outcome}", timeout.as_secs()))
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
    let found = line.Words().context("reading OCR words")?;
    let mut boxes = Vec::new();
    let mut words = Vec::new();
    for index in 0..found.Size().context("counting OCR words")? {
        let word = found.GetAt(index).context("reading an OCR word")?;
        let rect = word.BoundingRect().context("reading a word box")?;
        let text = word.Text().context("reading OCR word text")?.to_string_lossy();
        words.push(OcrWord { text, rect: union_to_screen(&[rect], scaling) });
        boxes.push(rect);
    }
    Ok(OcrLine { text, rect: union_to_screen(&boxes, scaling), words })
}

fn union_to_screen(boxes: &[WinRect], scaling: Scaling) -> Rect {
    let Some(union) = union_of(boxes) else {
        return Rect::new(scaling.origin.x, scaling.origin.y, 0, 0);
    };
    let left = floor_to_screen(union.X, scaling.x, scaling.origin.x);
    let top = floor_to_screen(union.Y, scaling.y, scaling.origin.y);
    let right = ceil_to_screen(union.X + union.Width, scaling.x, scaling.origin.x);
    let bottom = ceil_to_screen(union.Y + union.Height, scaling.y, scaling.origin.y);
    Rect::new(left as i32, top as i32, (right - left) as i32, (bottom - top) as i32)
}

fn union_of(boxes: &[WinRect]) -> Option<WinRect> {
    boxes.iter().copied().reduce(|union, word| {
        let left = union.X.min(word.X);
        let top = union.Y.min(word.Y);
        let right = (union.X + union.Width).max(word.X + word.Width);
        let bottom = (union.Y + union.Height).max(word.Y + word.Height);
        WinRect { X: left, Y: top, Width: right - left, Height: bottom - top }
    })
}

fn floor_to_screen(image: f32, factor: f64, origin: i32) -> f64 {
    f64::from(origin) + (f64::from(image) * factor).floor()
}

fn ceil_to_screen(image: f32, factor: f64, origin: i32) -> f64 {
    f64::from(origin) + (f64::from(image) * factor).ceil()
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
    use crate::frame::FrameSource;
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
        let frame = surface.to_frame(origin, FrameSource::Screen).unwrap();
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
        let word = line.words.iter().find(|word| word.text.contains("Deckhand")).unwrap();
        assert!(word.rect.x >= line.rect.x && word.rect.right() <= line.rect.right(), "{:?}", word.rect);
        assert!(word.rect.width < line.rect.width);
    }

    #[test]
    fn magnified_reading_maps_text_back_to_the_frame() {
        let frame = draw_text_frame(900, 160, 30, Point::new(100, 50));
        let lines = ocr_magnified(&frame).unwrap();
        let line = lines.iter().find(|line| line.text.contains("Deckhand")).unwrap();
        assert!(line.rect.x >= 100 + 20 && line.rect.x <= 100 + 60, "{:?}", line.rect);
        assert!(line.rect.y >= 50 && line.rect.bottom() <= 50 + 160, "{:?}", line.rect);
        assert_eq!(lines.iter().filter(|line| line.text.contains("Deckhand")).count(), 1);
    }

    #[test]
    fn tiles_cover_the_frame_with_overlap() {
        let tiles = tiles(5000, 1600, 1300, 64);
        assert_eq!(tiles.len(), 8);
        assert_eq!(tiles.iter().map(|tile| tile.x + tile.width).max(), Some(5000));
        assert_eq!(tiles.iter().map(|tile| tile.y + tile.height).max(), Some(1600));
        assert_eq!(tiles[1], Tile { x: 1236, y: 0, width: 1300, height: 1300 });
        assert_eq!(super::tiles(800, 600, 1300, 64), vec![Tile { x: 0, y: 0, width: 800, height: 600 }]);
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
    fn an_engine_exists_for_some_installed_language() {
        let _apartment = ComApartment::enter_any().unwrap();
        assert!(create_engine().is_ok());
        assert!(first_available_language().is_some());
    }

    #[test]
    fn recognition_finishes_before_its_timeout_and_a_zero_timeout_times_out() {
        let _apartment = ComApartment::enter_any().unwrap();
        let frame = draw_text_frame(400, 140, 10, Point::new(0, 0));
        let bitmap = software_bitmap(&frame.bgra, frame.width, frame.height).unwrap();
        let engine = create_engine().unwrap();
        let operation = engine.RecognizeAsync(&bitmap).unwrap();
        let result = finish_within(&operation, RECOGNITION_TIMEOUT).unwrap();
        assert!(result.Lines().unwrap().Size().unwrap() > 0);
        let slow = engine.RecognizeAsync(&bitmap).unwrap();
        match finish_within(&slow, Duration::ZERO) {
            Ok(_) => {}
            Err(error) => assert_eq!(error.code, deckhand_core::ErrorCode::Timeout),
        }
    }

    #[test]
    fn scaling_follows_each_axis_of_the_fit() {
        let frame = Frame::black(Point::new(-5, 9), 300, 200);
        let scaling = Scaling::between(&frame, Fit { width: 150, height: 99, scale: 2.0 });
        assert_eq!(scaling.origin, Point::new(-5, 9));
        assert!((scaling.x - 2.0).abs() < 1e-12 && (scaling.y - 200.0 / 99.0).abs() < 1e-12);
    }

    #[test]
    fn malformed_frames_are_rejected() {
        let mut frame = Frame::black(Point::new(0, 0), 5, 5);
        frame.bgra.clear();
        assert!(ocr(&frame).is_err());
    }
}
