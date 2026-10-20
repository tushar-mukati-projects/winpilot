use winpilot_core::{Element, ElementRef, Rect, Result, Role, Snapshot, WindowInfo, object_marks::readable_lines};

use crate::engine::Engine;

const MAIN_TEXT_SHARE: f64 = 0.2;
const BROWSER_CLASS_PREFIXES: [&str; 2] = ["Chrome_WidgetWin", "MozillaWindowClass"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WindowText {
    Text(String),
    OcrPage { page: Rect, lines: String },
}

pub(super) fn snapshot_text(engine: &Engine, snapshot: &Snapshot, max: usize) -> Result<WindowText> {
    let Some(main) = main_text_element(snapshot).and_then(|id| snapshot.get(id)) else {
        return Ok(WindowText::Text(collect_lines(&snapshot.elements, max)));
    };
    let text = uia_text(engine, main.id, max)?;
    Ok(chosen_text(snapshot, main, text, max))
}

fn chosen_text(snapshot: &Snapshot, main: &Element, text: String, max: usize) -> WindowText {
    if !readable_lines(&text).trim().is_empty() {
        return WindowText::Text(text);
    }
    let lines = collect_lines(&snapshot.elements, max);
    if is_browser_page(&snapshot.window, main) {
        return WindowText::OcrPage { page: main.rect, lines };
    }
    WindowText::Text(lines)
}

fn is_browser_page(window: &WindowInfo, main: &Element) -> bool {
    main.role == Role::Document && BROWSER_CLASS_PREFIXES.iter().any(|prefix| window.class_name.starts_with(prefix))
}

pub(super) fn uia_text(engine: &Engine, id: ElementRef, max: usize) -> Result<String> {
    let wanted = max.saturating_add(1);
    engine.uia.run(move |uia| uia.text(id, wanted))
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

#[cfg(test)]
mod tests {
    use winpilot_core::{WindowInfo, WindowState, text_limit::truncated};

    use super::*;
    use crate::test_fixture::{element, engine, snapshot};

    fn sized(mut element: Element, rect: Rect) -> Element {
        element.rect = rect;
        element
    }

    fn with_text_pattern(mut element: Element) -> Element {
        element.patterns.text = true;
        element
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
        let mut chrome = snapshot("Page - Google Chrome", elements);
        chrome.window.class_name = "Chrome_WidgetWin_1".to_string();
        chrome
    }

    #[test]
    fn an_empty_browser_page_is_read_by_ocr_of_the_page_not_from_the_browser_chrome() {
        let chrome = chrome_shaped_snapshot();
        let page = chrome.get(ElementRef(12)).unwrap();
        let lines = collect_lines(&chrome.elements, 1000);
        let ocr_page = WindowText::OcrPage { page: page.rect, lines: lines.clone() };
        assert_eq!(chosen_text(&chrome, page, " \u{FFFC} ".to_string(), 1000), ocr_page);
        assert_eq!(chosen_text(&chrome, page, "Jobs".to_string(), 1000), WindowText::Text("Jobs".to_string()));
        let mut desktop_app = chrome.clone();
        desktop_app.window.class_name = "Notepad".to_string();
        assert_eq!(chosen_text(&desktop_app, page, String::new(), 1000), WindowText::Text(lines));
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
        winpilot_desktop::windows()
            .ok()?
            .into_iter()
            .filter(|window| window.process.eq_ignore_ascii_case("chrome.exe"))
            .filter(|window| window.state != WindowState::Minimized)
            .filter(|window| !engine().guard.denies(window))
            .max_by_key(|window| window.rect.area())
    }

    #[test]
    #[ignore = "reads the user's Chrome window"]
    fn a_live_chrome_window_reads_its_page_document() {
        let Some(window) = visible_chrome_window() else { return };
        let snapshot = engine().snapshot(&window).unwrap();
        if !snapshot.elements.iter().any(|element| element.role == Role::Document) {
            return;
        }
        let chosen = main_text_element(&snapshot).expect("a Chrome window with a document picks it");
        let page = snapshot.elements.iter().find(|element| element.id == chosen).unwrap();
        assert_eq!(page.role, Role::Document);
        let WindowText::Text(text) =
            snapshot_text(engine(), &snapshot, winpilot_core::text_limit::DEFAULT_MAX_CHARS).unwrap()
        else {
            return;
        };
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
}
