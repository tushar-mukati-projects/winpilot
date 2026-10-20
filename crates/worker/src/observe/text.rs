mod ocr;
mod uia;

use winpilot_core::{
    Error, Rect, Result, Target, WindowInfo,
    object_marks::readable_lines,
    ocr::OcrLine,
    protocol::{OcrMode, TextReport, TextRequest, TextSource},
    text_limit::{char_limit, truncated},
};

use self::{
    ocr::{located_text, ocr_reading, plain_text},
    uia::{WindowText, snapshot_text, uia_text},
};
use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    frames, overlay,
    read_target::{ReadTarget, read_target},
    resolve,
};

const OCR_FALLBACK_BELOW: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Reading {
    text: String,
    source: TextSource,
    note: Option<&'static str>,
}

impl Reading {
    const fn uia(text: String) -> Self {
        Self { text, source: TextSource::Uia, note: None }
    }
}

#[derive(Clone, Copy)]
struct Asked {
    method: OcrMode,
    max: usize,
    joined: fn(&[OcrLine]) -> String,
}

pub(crate) fn run(engine: &Engine, request: &TextRequest, call: &CallScope) -> Result<TextReport> {
    let max = char_limit(request.max_chars);
    let asked = if request.coords {
        Asked { method: OcrMode::Always, max, joined: located_text }
    } else {
        Asked { method: request.ocr, max, joined: plain_text }
    };
    let reading = read(engine, request, asked, call)?;
    let (text, truncated) = truncated(readable_lines(&reading.text).into_owned(), max);
    Ok(TextReport { text, source: reading.source, truncated, note: reading.note.map(str::to_string) })
}

fn read(engine: &Engine, request: &TextRequest, asked: Asked, call: &CallScope) -> Result<Reading> {
    if let Some(region) = request.region {
        return ocr_reading(&frames::region(engine, region)?, asked.joined);
    }
    if let Some(window) = request.window.as_deref() {
        return scanned(engine, &resolve::window(engine, Some(window))?, asked);
    }
    match read_target(request.target.as_deref())? {
        ReadTarget::DefaultWindow => scanned(engine, &default_window(engine)?, asked),
        ReadTarget::Window(spec) => scanned(engine, &resolve::window(engine, Some(&spec))?, asked),
        ReadTarget::Element(target) => element_text(engine, &target, asked, call),
    }
}

fn scanned(engine: &Engine, window: &WindowInfo, asked: Asked) -> Result<Reading> {
    let reading = window_text(engine, window, asked)?;
    overlay::scan(engine, window);
    Ok(reading)
}

fn element_text(engine: &Engine, target: &Target, asked: Asked, call: &CallScope) -> Result<Reading> {
    let resolved = resolve::target(engine, target, call)?;
    let element =
        resolved.element_or_point_hit().cloned().ok_or_else(|| Error::invalid("the text target is not an element"))?;
    let ocr_element = || ocr_rect(engine, &resolved.window, element.rect, asked.joined);
    if asked.method == OcrMode::Always {
        return ocr_element();
    }
    let text = uia_text(engine, element.id, asked.max)?;
    if asked.method == OcrMode::Auto && readable_lines(&text).trim().is_empty() {
        return Ok(ocr_element().unwrap_or(Reading::uia(text)));
    }
    Ok(Reading::uia(text))
}

fn window_text(engine: &Engine, window: &WindowInfo, asked: Asked) -> Result<Reading> {
    engine.guard.check_window(window)?;
    if asked.method == OcrMode::Always {
        return ocr_reading(&frames::window_with_popups(engine, window)?, asked.joined);
    }
    let text = match snapshot_text(engine, &engine.snapshot(window)?, asked.max)? {
        WindowText::Text(text) => text,
        WindowText::OcrPage { page, lines } => {
            return Ok(blank_page(lines, asked.method, || ocr_rect(engine, window, page, plain_text)));
        }
    };
    if asked.method == OcrMode::Auto && readable_chars(&text) < OCR_FALLBACK_BELOW {
        return Ok(ocr_fallback(engine, window, text));
    }
    Ok(Reading::uia(text))
}

fn blank_page(lines: String, method: OcrMode, read_page: impl FnOnce() -> Result<Reading>) -> Reading {
    if method == OcrMode::Never {
        return Reading::uia(lines);
    }
    ocr_if_longer(read_page(), lines)
}

fn ocr_rect(engine: &Engine, window: &WindowInfo, rect: Rect, joined: fn(&[OcrLine]) -> String) -> Result<Reading> {
    frames::element(engine, window, rect).and_then(|captured| ocr_reading(&captured, joined))
}

fn readable_chars(text: &str) -> usize {
    readable_lines(text).trim().chars().count()
}

fn ocr_fallback(engine: &Engine, window: &WindowInfo, uia: String) -> Reading {
    let ocr = frames::window_with_popups(engine, window).and_then(|captured| ocr_reading(&captured, plain_text));
    ocr_if_longer(ocr, uia)
}

fn ocr_if_longer(ocr: Result<Reading>, uia: String) -> Reading {
    match ocr {
        Ok(reading) if readable_chars(&reading.text) > readable_chars(&uia) => reading,
        _ => Reading::uia(uia),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::{ErrorCode, WindowState};

    use super::*;
    use crate::test_fixture::{FixtureApp, engine, window};

    fn read_fixture(target: String, ocr: OcrMode, max_chars: Option<u32>) -> Result<TextReport> {
        let request = TextRequest { target: Some(target), ocr, max_chars, ..TextRequest::default() };
        run(engine(), &request, &CallScope::relaxed())
    }

    #[test]
    fn a_page_that_ocr_cannot_read_falls_back_to_the_window_lines() {
        let browser = window(0x10, "chrome.exe", "Page - Google Chrome", Rect::new(0, 0, 800, 600));
        let minimized = WindowInfo { state: WindowState::Minimized, ..browser };
        let lines = "Page\nAddress and search bar".to_string();
        for method in [OcrMode::Auto, OcrMode::Never] {
            let read_page = || ocr_rect(engine(), &minimized, Rect::new(0, 80, 800, 520), plain_text);
            assert_eq!(blank_page(lines.clone(), method, read_page), Reading::uia(lines.clone()), "{method:?}");
        }
    }

    #[test]
    fn a_page_that_ocr_reads_as_blank_keeps_the_window_lines() {
        let lines = "Page\nAddress and search bar".to_string();
        let ocr = |text: &str| -> Result<Reading> {
            Ok(Reading { text: text.to_string(), source: TextSource::Ocr, note: None })
        };
        assert_eq!(blank_page(lines.clone(), OcrMode::Auto, || ocr(" \n ")), Reading::uia(lines.clone()));
        let page = "Welcome back\nYour orders\nTrack a package";
        assert_eq!(blank_page(lines.clone(), OcrMode::Auto, || ocr(page)), ocr(page).unwrap());
        let never = blank_page(lines.clone(), OcrMode::Never, || panic!("read the page"));
        assert_eq!(never, Reading::uia(lines));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_window_text_comes_from_uia() {
        let app = FixtureApp::launch();
        let report = read_fixture(app.window.hwnd.to_string(), OcrMode::Auto, None).unwrap();
        assert_eq!(report.source, TextSource::Uia);
        assert!(report.text.contains("Clicks: 0"));
        assert!(report.text.contains("Open file"));
        assert!(!report.truncated);
        let short = read_fixture(app.window.hwnd.to_string(), OcrMode::Never, Some(5)).unwrap();
        assert_eq!((short.text.chars().count(), short.truncated), (5, true));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_element_text_by_selector() {
        let app = FixtureApp::launch();
        let selector = format!("window:\"{}\" >> id:2004", app.title());
        let report = read_fixture(selector, OcrMode::Auto, None).unwrap();
        assert_eq!((report.text.as_str(), report.source), ("Clicks: 0", TextSource::Uia));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_window_ocr() {
        let app = FixtureApp::launch();
        let report = match read_fixture(app.window.hwnd.to_string(), OcrMode::Always, None) {
            Err(error) if error.code == ErrorCode::Unsupported => return,
            result => result.unwrap(),
        };
        assert_eq!(report.source, TextSource::Ocr);
        assert!(report.text.to_lowercase().contains("click"));
    }
}
