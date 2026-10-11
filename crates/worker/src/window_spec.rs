use deckhand_core::{
    ElementRef, Error, Result, WindowInfo, WindowRef,
    refs::{looks_like_element_ref, looks_like_window_ref},
    selector::Selector,
};

const SELECTOR_KEYS: [&str; 9] = ["role", "name", "name~", "id", "class", "text", "window", "process", "nth"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowSpec {
    Handle(WindowRef),
    Element(ElementRef),
    Selector(Selector),
    Title(String),
}

pub fn parse(text: &str) -> Result<WindowSpec> {
    let text = text.trim();
    if looks_like_window_ref(text) {
        return text.parse().map(WindowSpec::Handle);
    }
    if looks_like_element_ref(text) {
        return text.parse().map(WindowSpec::Element);
    }
    if looks_like_title(text) {
        return Ok(WindowSpec::Title(text.to_string()));
    }
    let selector: Selector = text.parse()?;
    if !selector.has_window_terms() {
        return Err(Error::invalid(format!("'{text}' does not name a window; use window:\"title\" or process:name")));
    }
    Ok(WindowSpec::Selector(selector))
}

pub fn looks_like_title(text: &str) -> bool {
    !text.contains(':') || text.split_whitespace().filter_map(key_of).any(|key| !is_selector_key(key))
}

pub fn matches(spec: &WindowSpec, window: &WindowInfo) -> bool {
    match spec {
        WindowSpec::Selector(selector) => selector.matches_window(window),
        WindowSpec::Title(title) => window.title.to_lowercase().contains(&title.to_lowercase()),
        WindowSpec::Handle(hwnd) => window.hwnd == *hwnd,
        WindowSpec::Element(_) => false,
    }
}

pub fn describe(spec: &WindowSpec) -> String {
    match spec {
        WindowSpec::Handle(hwnd) => hwnd.to_string(),
        WindowSpec::Element(id) => id.to_string(),
        WindowSpec::Selector(selector) => selector.to_string(),
        WindowSpec::Title(title) => format!("\"{title}\""),
    }
}

fn key_of(word: &str) -> Option<&str> {
    word.split_once(':').map(|(key, _)| key)
}

fn is_selector_key(key: &str) -> bool {
    SELECTOR_KEYS.iter().any(|known| known.eq_ignore_ascii_case(key))
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, Rect};

    use super::*;
    use crate::test_fixture::window;

    fn title(text: &str) -> WindowSpec {
        WindowSpec::Title(text.to_string())
    }

    #[test]
    fn plain_text_and_refs() {
        assert_eq!(parse(" Untitled - Notepad ").unwrap(), title("Untitled - Notepad"));
        assert_eq!(parse("w42").unwrap(), WindowSpec::Handle(WindowRef(42)));
        assert_eq!(parse("e7").unwrap(), WindowSpec::Element(ElementRef(7)));
    }

    #[test]
    fn titles_with_colons_fall_back_to_title_matching() {
        assert_eq!(parse("C:\\a.txt - Notepad").unwrap(), title("C:\\a.txt - Notepad"));
        assert_eq!(parse("Re: lunch on Friday").unwrap(), title("Re: lunch on Friday"));
        assert_eq!(parse("Untitled: draft").unwrap(), title("Untitled: draft"));
    }

    #[test]
    fn window_selectors_parse_and_bad_ones_still_explain_themselves() {
        assert!(matches!(parse("process:notepad").unwrap(), WindowSpec::Selector(_)));
        assert!(matches!(parse("window:\"C:\\a.txt - Notepad\"").unwrap(), WindowSpec::Selector(_)));
        assert_eq!(parse("role:gizmo").unwrap_err().code, ErrorCode::InvalidArgument);
        let element_only = parse("name:Save").unwrap_err();
        assert!(element_only.message.contains("does not name a window"), "{}", element_only.message);
    }

    #[test]
    fn titles_match_case_insensitively() {
        let notepad = window(1, "notepad.exe", "C:\\A.txt - Notepad", Rect::new(0, 0, 10, 10));
        assert!(matches(&parse("c:\\a.txt").unwrap(), &notepad));
        assert!(matches(&WindowSpec::Handle(WindowRef(1)), &notepad));
        assert!(!matches(&title("Paint"), &notepad));
        assert!(!matches(&WindowSpec::Element(ElementRef(1)), &notepad));
        assert_eq!(describe(&title("Save As")), "\"Save As\"");
    }
}
