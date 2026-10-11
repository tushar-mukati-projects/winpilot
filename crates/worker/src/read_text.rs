use deckhand_capture::{Frame, ocr};
use deckhand_core::{
    Element, ElementRef, Error, Rect, Result, Role, Snapshot, Target, WindowInfo,
    protocol::{ReadTextArgs, TextMethod, TextReport, TextSource},
};

use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    frames, overlay,
    read_target::{ReadTarget, read_target},
    resolve,
};

const DEFAULT_MAX_CHARS: usize = 20_000;
const MAX_CHARS_LIMIT: usize = 200_000;
const OCR_FALLBACK_BELOW: usize = 40;
const MAIN_TEXT_SHARE: f64 = 0.2;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Reading {
    text: String,
    source: TextSource,
}

pub fn run(engine: &Engine, args: &ReadTextArgs, call: &CallScope) -> Result<TextReport> {
    args.validate()?;
    let max = max_chars(args.max_chars);
    let reading = read(engine, args, max, call)?;
    let (text, truncated) = truncate(reading.text, max);
    Ok(TextReport { text, source: reading.source, truncated })
}

fn read(engine: &Engine, args: &ReadTextArgs, max: usize, call: &CallScope) -> Result<Reading> {
    if let Some(region) = args.region {
        return ocr_frame(&frames::region(engine, region)?);
    }
    match read_target(args.target.as_deref())? {
        ReadTarget::DefaultWindow => scanned(engine, &default_window(engine)?, args.method, max),
        ReadTarget::Window(spec) => scanned(engine, &resolve::window(engine, Some(&spec))?, args.method, max),
        ReadTarget::Element(target) => element_text(engine, &target, args.method, max, call),
    }
}

fn scanned(engine: &Engine, window: &WindowInfo, method: TextMethod, max: usize) -> Result<Reading> {
    let reading = window_text(engine, window, method, max)?;
    overlay::scan(engine, window);
    Ok(reading)
}

fn element_text(engine: &Engine, target: &Target, method: TextMethod, max: usize, call: &CallScope) -> Result<Reading> {
    let resolved = resolve::target(engine, target, call)?;
    let element =
        resolved.element().cloned().ok_or_else(|| Error::invalid("the read_text target is not an element"))?;
    let ocr_element = || frames::element(engine, &resolved.window, element.rect).and_then(|frame| ocr_frame(&frame));
    if method == TextMethod::Ocr {
        return ocr_element();
    }
    let text = uia_text(engine, element.id, max)?;
    if method == TextMethod::Auto && text.trim().is_empty() {
        return Ok(ocr_element().unwrap_or(Reading { text, source: TextSource::Uia }));
    }
    Ok(Reading { text, source: TextSource::Uia })
}

fn window_text(engine: &Engine, window: &WindowInfo, method: TextMethod, max: usize) -> Result<Reading> {
    engine.guard.check_window(window)?;
    if method == TextMethod::Ocr {
        return ocr_frame(&frames::window(engine, window)?);
    }
    let text = snapshot_text(engine, &engine.snapshot(window)?, max)?;
    if method == TextMethod::Auto && text.trim().chars().count() < OCR_FALLBACK_BELOW {
        return Ok(ocr_fallback(engine, window, text));
    }
    Ok(Reading { text, source: TextSource::Uia })
}

fn ocr_fallback(engine: &Engine, window: &WindowInfo, uia: String) -> Reading {
    let longer = |reading: &Reading| reading.text.trim().chars().count() > uia.trim().chars().count();
    match frames::window(engine, window).and_then(|frame| ocr_frame(&frame)) {
        Ok(reading) if longer(&reading) => reading,
        _ => Reading { text: uia, source: TextSource::Uia },
    }
}

fn snapshot_text(engine: &Engine, snapshot: &Snapshot, max: usize) -> Result<String> {
    if let Some(id) = main_text_element(snapshot) {
        let text = uia_text(engine, id, max)?;
        if !text.trim().is_empty() {
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

fn ocr_frame(frame: &Frame) -> Result<Reading> {
    let lines = ocr(frame)?;
    let text = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
    Ok(Reading { text, source: TextSource::Ocr })
}

fn max_chars(requested: Option<u32>) -> usize {
    requested.map_or(DEFAULT_MAX_CHARS, |max| max as usize).clamp(1, MAX_CHARS_LIMIT)
}

fn truncate(mut text: String, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((cut, _)) => {
            text.truncate(cut);
            (text, true)
        }
        None => (text, false),
    }
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

    fn read_fixture(target: String, method: TextMethod, max_chars: Option<u32>) -> Result<TextReport> {
        run(engine(), &ReadTextArgs { target: Some(target), region: None, method, max_chars }, &CallScope::relaxed())
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
        let chosen = main_text_element(&snapshot).expect("a visible Chrome window has a page document");
        let page = snapshot.elements.iter().find(|element| element.id == chosen).unwrap();
        assert_eq!(page.role, Role::Document);
        let text = snapshot_text(engine(), &snapshot, DEFAULT_MAX_CHARS).unwrap();
        let address = page.value.as_deref().unwrap_or_default().trim();
        assert!(
            address.is_empty() || text.trim() != address,
            "read_text returned the page address instead of its text"
        );
    }

    #[test]
    fn max_chars_defaults_and_clamps() {
        assert_eq!(max_chars(None), 20_000);
        assert_eq!(max_chars(Some(0)), 1);
        assert_eq!(max_chars(Some(50)), 50);
        assert_eq!(max_chars(Some(u32::MAX)), 200_000);
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("héllo".to_string(), 2), ("hé".to_string(), true));
        assert_eq!(truncate("日本語".to_string(), 3), ("日本語".to_string(), false));
        assert_eq!(truncate("hi".to_string(), 5), ("hi".to_string(), false));
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
        assert_eq!(truncate(text, 20), ("line 1\nline 2\nline 3".to_string(), true));
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
        let report = read_fixture(app.window.hwnd.to_string(), TextMethod::Auto, None).unwrap();
        assert_eq!(report.source, TextSource::Uia);
        assert!(report.text.contains("Clicks: 0"));
        assert!(report.text.contains("Open file"));
        assert!(!report.truncated);
        let short = read_fixture(app.window.hwnd.to_string(), TextMethod::Uia, Some(5)).unwrap();
        assert_eq!((short.text.chars().count(), short.truncated), (5, true));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_element_text_by_selector() {
        let Some(app) = FixtureApp::launch() else { return };
        let selector = format!("window:\"{}\" >> id:2004", app.title());
        let report = read_fixture(selector, TextMethod::Auto, None).unwrap();
        assert_eq!((report.text.as_str(), report.source), ("Clicks: 0", TextSource::Uia));
    }

    #[test]
    #[ignore = "opens the off-screen fixture window"]
    fn fixture_window_ocr() {
        let Some(app) = FixtureApp::launch() else { return };
        let report = match read_fixture(app.window.hwnd.to_string(), TextMethod::Ocr, None) {
            Err(error) if error.code == ErrorCode::Unsupported => return,
            result => result.unwrap(),
        };
        assert_eq!(report.source, TextSource::Ocr);
        assert!(report.text.to_lowercase().contains("click"));
    }
}
