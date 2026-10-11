use std::time::Instant;

use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::core::Result;

use crate::util::{Frame, median, ms_since};

pub struct OcrRow {
    pub name: String,
    pub cold_ms: f64,
    pub warm_ms: f64,
    pub bitmap_ms: f64,
    pub lines: u32,
    pub words: u32,
}

pub struct OcrReport {
    pub languages: Vec<String>,
    pub max_dim: u32,
    pub engine_ms: f64,
    pub rows: Vec<OcrRow>,
}

fn to_bitmap(f: &Frame) -> Result<SoftwareBitmap> {
    let buf = CryptographicBuffer::CreateFromByteArray(&f.bgra)?;
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(&buf, BitmapPixelFormat::Bgra8, f.width as i32, f.height as i32, BitmapAlphaMode::Ignore)
}

fn recognize(engine: &OcrEngine, bmp: &SoftwareBitmap) -> Result<(u32, u32)> {
    let res = engine.RecognizeAsync(bmp)?.join()?;
    let lines = res.Lines()?;
    let n = lines.Size()?;
    let mut words = 0;
    for i in 0..n {
        words += lines.GetAt(i)?.Words()?.Size()?;
    }
    Ok((n, words))
}

pub fn bench(frames: &[(&str, &Frame)], runs: usize) -> Result<OcrReport> {
    let langs = OcrEngine::AvailableRecognizerLanguages()?;
    let mut languages = Vec::new();
    for i in 0..langs.Size()? {
        languages.push(langs.GetAt(i)?.LanguageTag()?.to_string());
    }
    let max_dim = OcrEngine::MaxImageDimension()?;
    let t = Instant::now();
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let engine_ms = ms_since(t);
    let mut rows = Vec::new();
    for (name, f) in frames {
        if f.width.max(f.height) > max_dim {
            rows.push(OcrRow { name: format!("{name} {}x{} skipped (> MaxImageDimension)", f.width, f.height), cold_ms: f64::NAN, warm_ms: f64::NAN, bitmap_ms: f64::NAN, lines: 0, words: 0 });
            continue;
        }
        let mut times = Vec::new();
        let mut bms = Vec::new();
        let mut counts = (0, 0);
        for _ in 0..runs {
            let t = Instant::now();
            let bmp = to_bitmap(f)?;
            bms.push(ms_since(t));
            counts = recognize(&engine, &bmp)?;
            times.push(ms_since(t));
        }
        let cold = times[0];
        let warm = median(&times[1.min(times.len() - 1)..]);
        rows.push(OcrRow { name: format!("{name} {}x{}", f.width, f.height), cold_ms: cold, warm_ms: warm, bitmap_ms: median(&bms), lines: counts.0, words: counts.1 });
    }
    Ok(OcrReport { languages, max_dim, engine_ms, rows })
}
