use deckhand_capture::ocr;
use deckhand_core::{
    Element, ElementRef, Error, Rect, Result, Role, Snapshot, Target, WindowInfo,
    object_marks::readable_lines,
    ocr::{OcrLine, line_with_coords},
    protocol::{OcrMode, TextReport, TextRequest, TextSource},
    text_limit::{char_limit, truncated},
};

use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    frames::{self, Captured},
    overlay,
    read_target::{ReadTarget, read_target},
    resolve,
};

const OCR_FALLBACK_BELOW: usize = 40;
const MAIN_TEXT_SHARE: f64 = 0.2;

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

pub(crate) fn run(engine: &Engine, request: &TextRequest, call: &CallScope) -> Result<TextReport> {
    let max = char_limit(request.max_chars);
    let reading = read(engine, request, max, call)?;
    let (text, truncated) = truncated(readable_lines(&reading.text).into_owned(), max);
    Ok(TextReport { text, source: reading.source, truncated, note: reading.note.map(str::to_string) })
}

fn read(engine: &Engine, request: &TextRequest, max: usize, call: &CallScope) -> Result<Reading> {
    if request.coords {
        return located_lines(engine, request, call);
    }
    if let Some(region) = request.region {
        return ocr_capture(&frames::region(engine, region)?);
    }
    if let Some(window) = request.window.as_deref() {
        return scanned(engine, &resolve::window(engine, Some(window))?, request.ocr, max);
    }
    match read_target(request.target.as_deref())? {
        ReadTarget::DefaultWindow => scanned(engine, &default_window(engine)?, request.ocr, max),
        ReadTarget::Window(spec) => scanned(engine, &resolve::window(engine, Some(&spec))?, request.ocr, max),
        ReadTarget::Element(target) => element_text(engine, &target, request.ocr, max, call),
    }
}

fn located_lines(engine: &Engine, request: &TextRequest, call: &CallScope) -> Result<Reading> {
    if let Some(region) = request.region {
        return ocr_with_coords(&frames::region(engine, region)?);
    }
    if let Some(window) = request.window.as_deref() {
        return window_lines(engine, &resolve::window(engine, Some(window))?);
    }
    match read_target(request.target.as_deref())? {
        ReadTarget::DefaultWindow => window_lines(engine, &default_window(engine)?),
        ReadTarget::Window(spec) => window_lines(engine, &resolve::window(engine, Some(&spec))?),
        ReadTarget::Element(target) => element_lines(engine, &target, call),
    }
}

fn window_lines(engine: &Engine, window: &WindowInfo) -> Result<Reading> {
    let reading = ocr_with_coords(&frames::window(engine, window)?)?;
    overlay::scan(engine, window);
    Ok(reading)
}

fn element_lines(engine: &Engine, target: &Target, call: &CallScope) -> Result<Reading> {
    let resolved = resolve::target(engine, target, call)?;
    let element = resolved.element_or_point_hit().ok_or_else(|| Error::invalid("the text target is not an element"))?;
    ocr_with_coords(&frames::element(engine, &resolved.window, element.rect)?)
}

fn ocr_with_coords(captured: &Captured) -> Result<Reading> {
    let lines = ocr(&captured.frame)?;
    Ok(Reading { text: located_text(&lines), source: TextSource::Ocr, note: captured.note })
}

fn located_text(lines: &[OcrLine]) -> String {
    lines.iter().map(line_with_coords).collect::<Vec<_>>().join("\n")
}

fn scanned(engine: &Engine, window: &WindowInfo, method: OcrMode, max: usize) -> Result<Reading> {
    let reading = window_text(engine, window, method, max)?;
    overlay::scan(engine, window);
    Ok(reading)
}

fn element_text(engine: &Engine, target: &Target, method: OcrMode, max: usize, call: &CallScope) -> Result<Reading> {
    let resolved = resolve::target(engine, target, call)?;
    let element =
        resolved.element_or_point_hit().cloned().ok_or_else(|| Error::invalid("the text target is not an element"))?;
    let ocr_element =
        || frames::element(engine, &resolved.window, element.rect).and_then(|captured| ocr_capture(&captured));
    if method == OcrMode::Always {
        return ocr_element();
    }
    let text = uia_text(engine, element.id, max)?;
    if method == OcrMode::Auto && readable_lines(&text).trim().is_empty() {
        return Ok(ocr_element().unwrap_or(Reading::uia(text)));
    }
    Ok(Reading::uia(text))
}

fn window_text(engine: &Engine, window: &WindowInfo, method: OcrMode, max: usize) -> Result<Reading> {
    engine.guard.check_window(window)?;
    if method == OcrMode::Always {
        return ocr_capture(&frames::window(engine, window)?);
    }
    let text = snapshot_text(engine, &engine.snapshot(window)?, max)?;
    if method == OcrMode::Auto && readable_chars(&text) < OCR_FALLBACK_BELOW {
        return Ok(ocr_fallback(engine, window, text));
    }
    Ok(Reading::uia(text))
}

fn readable_chars(text: &str) -> usize {
    readable_lines(text).trim().chars().count()
}

fn ocr_fallback(engine: &Engine, window: &WindowInfo, uia: String) -> Reading {
    let longer = |reading: &Reading| readable_chars(&reading.text) > readable_chars(&uia);
    match frames::window(engine, window).and_then(|captured| ocr_capture(&captured)) {
        Ok(reading) if longer(&reading) => reading,
        _ => Reading::uia(uia),
    }
}

fn snapshot_text(engine: &Engine, snapshot: &Snapshot, max: usize) -> Result<String> {
    if let Some(id) = main_text_element(snapshot) {
        let text = uia_text(engine, id, max)?;
        if !readable_lines(&text).trim().is_empty() {
            return Ok(text);
        }
    }
    Ok(collect_lines(&snapshot.elements, max))
}

fn main_text_element(snapshot: &Snapshot) -> Option<ElementRef> {
    let window = snapshot.window.rect;
    let candidates = || snapshot.elements.iter().filter(|element| is_main_text_candidate(element, window));
    let largest = |found: Vec<&Element>| found.into_iter().max_by_key(|element| element.rect.area()).map(|e| e.id);
    let documents: Vec<&Element> = candidates().filter(|element| element.role == Role::Document).collect();
    focused_document(snapshot, &documents).or_else(|| largest(documents)).or_else(|| {
        largest(candidates().filter(|element| element.role == Role::Edit && element.patterns.text).collect())
    })
}

fn focused_document(snapshot: &Snapshot, documents: &[&Element]) -> Option<ElementRef> {
    let focused = snapshot.index_of(snapshot.focused?)?;
    let is_document = |element: &Element| documents.iter().any(|document| document.id == element.id);
    let focused_element = snapshot.elements.get(focused)?;
    if is_document(focused_element) {
        return Some(focused_element.id);
    }
    snapshot.ancestors(focused).into_iter().rev().find(|ancestor| is_document(ancestor)).map(|ancestor| ancestor.id)
}

fn is_main_text_candidate(element: &Element, window: Rect) -> bool {
    let known_empty = element.value.as_deref().is_some_and(str::is_empty);
    element.is_visible()
        && !element.state.password
        && !known_empty
        && element.rect.area() as f64 >= window.area() as f64 * MAIN_TEXT_SHARE
}

fn collect_lines(elements: &[Element], max: usize) -> String {
    let pieces = elements.iter().filter(|element| element.is_visible()).flat_map(element_texts).map(str::trim);
    let mut lines: Vec<&str> = Vec::new();
    let mut chars = 0;
    for piece in pieces.filter(|piece| !piece.is_empty()) {
        if lines.last() == Some(&piece) {
            continue;
        }
        chars += piece.chars().count() + usize::from(!lines.is_empty());
        lines.push(piece);
        if chars > max {
            break;
        }
    }
    lines.join("\n")
}

fn element_texts(element: &Element) -> impl Iterator<Item = &str> {
    let value = element.value.as_deref().filter(|_| !element.state.password);
    std::iter::once(element.name.as_str()).chain(value)
}

fn uia_text(engine: &Engine, id: ElementRef, max: usize) -> Result<String> {
    let wanted = max.saturating_add(1);
    engine.uia.run(move |uia| uia.text(id, wanted))
}

fn ocr_capture(captured: &Captured) -> Result<Reading> {
    let lines = ocr(&captured.frame)?;
    let text = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
    Ok(Reading { text, source: TextSource::Ocr, note: captured.note })
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;
    use crate::test_fixture::{FixtureApp, element, engine, snapshot};

    fn sized(mut element: Element, rect: Rect) -> Element {
        element.rect = rect;
        element
    }

    fn with_text_pattern(mut element: Element) -> Element {
        element.patterns.text = true;
        element
    }

    fn read_fixture(target: String, ocr: OcrMode, max_chars: Option<u32>) -> Result<TextReport> {
        let request = TextRequest { target: Some(target), ocr, max_chars, ..TextRequest::default() };
        run(engine(), &request, &CallScope::relaxed())
    }

    fn with_value(mut element: Element, value: &str) -> Element {
        element.value = Some(value.to_string());
        element.patterns.value = true;
        element
    }

    fn chrome_shaped_snapshot() -> Snapshot {
        let mut omnibox = with_text_pattern(with_value(element(8, 6, Role::Edit, "Address and search bar"), "url"));
        omnibox.rect = Rect::new(120, 40, 600, 30);
        let page = with_text_pattern(with_value(element(12, 7, Role::Document, "Page"), "https://example.com/"));
        let frame = with_text_pattern(with_value(element(48, 11, Role::Document, "Frame"), "https://ads.example/"));
        let elements = vec![
            sized(element(1, 0, Role::Window, "Page - Google Chrome"), Rect::new(0, 0, 800, 600)),
            sized(element(3, 1, Role::Pane, ""), Rect::new(0, 0, 800, 600)),
            sized(element(5, 2, Role::Tab, "Page"), Rect::new(0, 0, 200, 30)),
            omnibox,
            sized(element(10, 6, Role::Pane, ""), Rect::new(0, 80, 800, 520)),
            sized(page, Rect::new(0, 80, 800, 520)),
            sized(frame, Rect::new(500, 300, 300, 200)),
        ];
        snapshot("Page - Google Chrome", elements)
    }

    #[test]
    fn located_lines_end_with_their_screen_rectangles() {
        let lines = [
            OcrLine { text: "Import".to_string(), rect: Rect::new(10, 20, 60, 16), words: Vec::new() },
            OcrLine { text: "Export video".to_string(), rect: Rect::new(-1900, 40, 120, 16), words: Vec::new() },
        ];
        assert_eq!(located_text(&lines), "Import @10,20,60x16\nExport video @-1900,40,120x16");
        assert_eq!(located_text(&[]), "");
    }

    #[test]
    fn a_browser_page_document_wins_over_the_omnibox_and_frames() {
        assert_eq!(main_text_element(&chrome_shaped_snapshot()), Some(ElementRef(12)));
    }

    #[test]
    fn browser_chrome_alone_has_no_main_text() {
        let mut chrome_only = chrome_shaped_snapshot();
        chrome_only.elements.retain(|element| element.role != Role::Document);
        assert_eq!(main_text_element(&chrome_only), None);
    }

    fn visible_chrome_window() -> Option<WindowInfo> {
        deckhand_desktop::windows()
            .ok()?
            .into_iter()
            .filter(|window| window.process.eq_ignore_ascii_case("chrome.exe"))
            .filter(|window| window.state != deckhand_core::WindowState::Minimized)
            .filter(|window| !engine().guard.denies(window))
            .max_by_key(|window| window.rect.area())
    }

    #[test]
    fn a_live_chrome_window_reads_its_page_document() {
        let Some(window) = visible_chrome_window() else { return };
        let snapshot = engine().snapshot(&window).unwrap();
        if !snapshot.elements.iter().any(|element| element.role == Role::Document) {
            return;
        }
        let chosen = main_text_element(&snapshot).expect("a Chrome window with a document picks it");
        let page = snapshot.elements.iter().find(|element| element.id == chosen).unwrap();
        assert_eq!(page.role, Role::Document);
        let text = snapshot_text(engine(), &snapshot, deckhand_core::text_limit::DEFAULT_MAX_CHARS).unwrap();
        let address = page.value.as_deref().unwrap_or_default().trim();
        assert!(
            address.is_empty() || text.trim() != address,
            "text mode returned the page address instead of its text"
        );
    }

    #[test]
    fn a_slack_style_tree_reads_without_object_marks() {
        let elements = vec![
            element(1, 0, Role::Text, "general"),
            element(2, 1, Role::Text, "\u{FFFC}"),
            element(3, 1, Role::Text, "hi \u{FFFC}there"),
        ];
        assert_eq!(readable_lines(&collect_lines(&elements, 1000)), "general\nhi there");
    }

    #[test]
    fn lines_skip_empties_repeats_hidden_and_passwords() {
        let mut hidden = element(4, 1, Role::Text, "Hidden");
        hidden.state.offscreen = true;
        let mut password = element(6, 1, Role::Edit, "Password");
        password.value = Some("hunter2".to_string());
        password.state.password = true;
        let mut name = element(5, 1, Role::Edit, "Name");
        name.value = Some("Ada".to_string());
        let elements = vec![
            element(1, 0, Role::Window, "Form"),
            element(2, 1, Role::Text, "  "),
            element(3, 1, Role::Text, "Form"),
            hidden,
            name,
            password,
            element(7, 1, Role::Text, "Name"),
        ];
        assert_eq!(collect_lines(&elements, 1000), "Form\nName\nAda\nPassword\nName");
    }

    #[test]
    fn lines_stop_once_past_the_limit() {
        let elements: Vec<Element> = (1..=50).map(|id| element(id, 1, Role::Text, &format!("line {id}"))).collect();
        let text = collect_lines(&elements, 20);
        assert_eq!(text, "line 1\nline 2\nline 3\nline 4");
        assert_eq!(truncated(text, 20), ("line 1\nline 2\nline 3".to_string(), true));
        assert_eq!(collect_lines(&elements[..3], 20), "line 1\nline 2\nline 3");
    }

    #[test]
    fn the_document_holding_the_focus_wins_over_a_larger_one() {
        let mut focused_field = element(4, 2, Role::Edit, "Reply");
        focused_field.state.focused = true;
        let elements = vec![
            sized(element(1, 0, Role::Document, "Inbox"), Rect::new(0, 0, 800, 500)),
            sized(element(2, 1, Role::Text, "Message"), Rect::new(0, 0, 800, 100)),
            sized(element(3, 0, Role::Document, "Compose"), Rect::new(0, 400, 600, 200)),
            sized(focused_field, Rect::new(10, 420, 500, 40)),
        ];
        let mut snapshot = snapshot("Mail", elements);
        assert_eq!(main_text_element(&snapshot), Some(ElementRef(1)));
        snapshot.focused = Some(ElementRef(4));
        assert_eq!(main_text_element(&snapshot), Some(ElementRef(3)));
        snapshot.focused = Some(ElementRef(1));
        assert_eq!(main_text_element(&snapshot), Some(ElementRef(1)));
        snapshot.focused = Some(ElementRef(2));
        assert_eq!(main_text_element(&snapshot), Some(ElementRef(1)));
    }

    #[test]
    fn prefers_the_largest_visible_document() {
        let elements = vec![
            sized(element(1, 0, Role::Document, "Small"), Rect::new(0, 0, 10, 10)),
            sized(element(2, 0, Role::Document, "Big"), Rect::new(0, 0, 400, 400)),
            sized(with_text_pattern(element(3, 0, Role::Edit, "Editor")), Rect::new(0, 0, 800, 600)),
        ];
        assert_eq!(main_text_element(&snapshot("Doc", elements)), Some(ElementRef(2)));
    }

    #[test]
    fn small_or_empty_documents_do_not_count() {
        let mut empty = sized(element(1, 0, Role::Document, "Empty"), Rect::new(0, 0, 800, 600));
        empty.value = Some(String::new());
        let elements = vec![empty, sized(element(2, 0, Role::Document, "Notes"), Rect::new(0, 0, 100, 100))];
        assert_eq!(main_text_element(&snapshot("Form", elements)), None);
    }

    #[test]
    fn falls_back_to_a_large_edit_with_the_text_pattern() {
        let elements = vec![
            sized(with_text_pattern(element(1, 0, Role::Edit, "Tiny")), Rect::new(0, 0, 50, 20)),
            sized(element(2, 0, Role::Edit, "No pattern"), Rect::new(0, 0, 800, 600)),
            sized(with_text_pattern(element(3, 0, Role::Edit, "Body")), Rect::new(0, 50, 800, 500)),
        ];
        assert_eq!(main_text_element(&snapshot("Editor", elements)), Some(ElementRef(3)));
        let small_only = vec![sized(with_text_pattern(element(1, 0, Role::Edit, "Tiny")), Rect::new(0, 0, 50, 20))];
        assert_eq!(main_text_element(&snapshot("Form", small_only)), None);
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_window_text_comes_from_uia() {
        let Some(app) = FixtureApp::launch() else { return };
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
        let Some(app) = FixtureApp::launch() else { return };
        let selector = format!("window:\"{}\" >> id:2004", app.title());
        let report = read_fixture(selector, OcrMode::Auto, None).unwrap();
        assert_eq!((report.text.as_str(), report.source), ("Clicks: 0", TextSource::Uia));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_window_ocr() {
        let Some(app) = FixtureApp::launch() else { return };
        let report = match read_fixture(app.window.hwnd.to_string(), OcrMode::Always, None) {
            Err(error) if error.code == ErrorCode::Unsupported => return,
            result => result.unwrap(),
        };
        assert_eq!(report.source, TextSource::Ocr);
        assert!(report.text.to_lowercase().contains("click"));
    }
}
