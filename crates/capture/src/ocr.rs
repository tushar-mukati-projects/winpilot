mod contrast;
mod lines;
mod tiles;

use std::sync::mpsc;
use std::time::Duration;

use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Security::Cryptography::CryptographicBuffer;
use windows_future::{AsyncOperationCompletedHandler, IAsyncOperation};
use winpilot_core::{Context, Error, Rect, Result, ocr::OcrLine};
use winpilot_win32::ComApartment;

use self::contrast::ink_contrast;
use self::lines::{Scaling, collect_lines};
use self::tiles::tiles;
use crate::frame::Frame;
use crate::pixels::Block;
use crate::resize::{fit, fitted_pixels, resize_bgra};

const RECOGNITION_TIMEOUT: Duration = Duration::from_secs(10);
const MAGNIFICATION: u32 = 2;
const TILE_OVERLAP: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrPass {
    Plain,
    Magnified,
    Contrasted,
}

pub fn ocr(frame: &Frame, pass: OcrPass) -> Result<Vec<OcrLine>> {
    if !frame.is_well_formed() {
        return Err(Error::invalid("frame size does not match its pixel buffer"));
    }
    let _apartment = ComApartment::enter_any()?;
    let engine = create_engine()?;
    let limit = OcrEngine::MaxImageDimension().context("reading the OCR size limit")?;
    match pass {
        OcrPass::Plain => {
            let fitted = fit(frame.width, frame.height, limit, limit);
            let pixels = fitted_pixels(frame, fitted)?;
            recognized(&engine, &pixels, fitted.width, fitted.height, Scaling::between(frame, fitted))
        }
        OcrPass::Magnified => magnified_lines(&engine, frame, limit, |_| {}),
        OcrPass::Contrasted => magnified_lines(&engine, frame, limit, ink_contrast),
    }
}

fn magnified_lines(engine: &OcrEngine, frame: &Frame, limit: u32, prepare: fn(&mut [u8])) -> Result<Vec<OcrLine>> {
    let side = (limit / MAGNIFICATION).max(TILE_OVERLAP * 2);
    let mut lines: Vec<OcrLine> = Vec::new();
    for tile in tiles(frame.width, frame.height, side, TILE_OVERLAP) {
        for line in magnified_tile_lines(engine, frame, tile, prepare)? {
            if !lines.iter().any(|seen| same_line(seen, &line)) {
                lines.push(line);
            }
        }
    }
    Ok(lines)
}

fn magnified_tile_lines(
    engine: &OcrEngine,
    frame: &Frame,
    tile: Block,
    prepare: fn(&mut [u8]),
) -> Result<Vec<OcrLine>> {
    let area = Rect::new(
        frame.origin.x + tile.x as i32,
        frame.origin.y + tile.y as i32,
        tile.width as i32,
        tile.height as i32,
    );
    let cropped = frame.crop(area).ok_or_else(|| Error::internal("the OCR tile lies outside the frame"))?;
    let (width, height) = (tile.width * MAGNIFICATION, tile.height * MAGNIFICATION);
    let mut pixels = resize_bgra(&cropped.bgra, (tile.width, tile.height), (width, height))?;
    prepare(&mut pixels);
    let shrink = 1.0 / f64::from(MAGNIFICATION);
    recognized(engine, &pixels, width, height, Scaling { origin: cropped.origin, x: shrink, y: shrink })
}

fn recognized(engine: &OcrEngine, bgra: &[u8], width: u32, height: u32, scaling: Scaling) -> Result<Vec<OcrLine>> {
    let bitmap = software_bitmap(bgra, width, height)?;
    let operation = engine.RecognizeAsync(&bitmap).context("starting OCR")?;
    collect_lines(&finish_within(&operation, RECOGNITION_TIMEOUT)?, scaling)
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
    use windows::core::{Owned, w};
    use winpilot_core::Point;

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
        let mut text: Vec<u16> = "WinPilot 2026".encode_utf16().collect();
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
        let lines = ocr(&frame, OcrPass::Plain).unwrap();
        eprintln!("timing: OCR of a 900x160 synthetic frame: {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        let line = lines.iter().find(|line| line.text.contains("WinPilot")).unwrap();
        assert!(line.text.contains("2026"));
        assert!(line.rect.x >= 100 + 20 && line.rect.x <= 100 + 60, "{:?}", line.rect);
        assert!(line.rect.y >= 50 && line.rect.bottom() <= 50 + 160, "{:?}", line.rect);
        assert!(line.rect.width > 300);
        let word = line.words.iter().find(|word| word.text.contains("WinPilot")).unwrap();
        assert!(word.rect.x >= line.rect.x && word.rect.right() <= line.rect.right(), "{:?}", word.rect);
        assert!(word.rect.width < line.rect.width);
    }

    #[test]
    fn magnified_reading_maps_text_back_to_the_frame() {
        let frame = draw_text_frame(900, 160, 30, Point::new(100, 50));
        let lines = ocr(&frame, OcrPass::Magnified).unwrap();
        let line = lines.iter().find(|line| line.text.contains("WinPilot")).unwrap();
        assert!(line.rect.x >= 100 + 20 && line.rect.x <= 100 + 60, "{:?}", line.rect);
        assert!(line.rect.y >= 50 && line.rect.bottom() <= 50 + 160, "{:?}", line.rect);
        assert_eq!(lines.iter().filter(|line| line.text.contains("WinPilot")).count(), 1);
    }

    #[test]
    fn oversized_frames_are_downscaled_and_mapped_back() {
        let limit = OcrEngine::MaxImageDimension().unwrap();
        if limit > 16_000 {
            return;
        }
        let width = limit * 2;
        let frame = draw_text_frame(width, 300, limit as i32, Point::new(-50, 0));
        let lines = ocr(&frame, OcrPass::Plain).unwrap();
        let line = lines.iter().find(|line| line.text.contains("WinPilot")).unwrap();
        let expected_left = limit as i32 - 50;
        assert!((line.rect.x - expected_left).abs() < 60, "{:?} vs {expected_left}", line.rect);
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
            Err(error) => assert_eq!(error.code, winpilot_core::ErrorCode::Timeout),
        }
    }

    #[test]
    fn malformed_frames_are_rejected() {
        let mut frame = Frame::black(Point::new(0, 0), 5, 5);
        frame.bgra.clear();
        for pass in [OcrPass::Plain, OcrPass::Magnified, OcrPass::Contrasted] {
            assert!(ocr(&frame, pass).is_err());
        }
    }
}
