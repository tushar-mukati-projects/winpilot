use winpilot_core::{Element, Result, Snapshot, format::window_line, ocr::exposes_no_uia};

use super::{
    check::{Check, Presence, unless_gone},
    ocr_look::OcrLook,
};
use crate::{engine::Engine, resolve, watch::Watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextHit {
    Title,
    Element,
}

pub(crate) fn check(engine: &Engine, text: &str, scope: Option<&str>, ocr: &mut OcrLook<'_>) -> Result<Check> {
    let Some(window) = unless_gone(resolve::window(engine, scope))? else {
        return Ok(Check::unmet(format!("no window to search for \"{text}\"")));
    };
    let Some(snapshot) = unless_gone(engine.snapshot(&window))? else {
        return Ok(Check::unmet(format!("{} closed before \"{text}\" showed", window_line(&window))));
    };
    let check = text_check(&snapshot, text);
    if check.met || !exposes_no_uia(&snapshot) {
        return Ok(check);
    }
    let read = ocr.presence(engine, std::slice::from_ref(&snapshot.window), text, None)?;
    Ok(read_by_ocr(text, read).unwrap_or(check))
}

fn read_by_ocr(text: &str, read: Presence) -> Option<Check> {
    read.shown.map(|_| Check::met(format!("\"{text}\" is showing (read by OCR)")))
}

pub(crate) fn watch(engine: &Engine, scope: Option<&str>) -> Watch {
    resolve::window(engine, scope).map(|window| Watch::window(&window)).unwrap_or_default()
}

fn text_check(snapshot: &Snapshot, text: &str) -> Check {
    match find_text(snapshot, text) {
        Some(TextHit::Title) => {
            Check::met(format!("the title of {} contains \"{text}\"", window_line(&snapshot.window)))
        }
        Some(TextHit::Element) => Check::met(format!("\"{text}\" is showing")),
        None => Check::unmet(format!("\"{text}\" is not showing in {}", window_line(&snapshot.window))),
    }
}

fn find_text(snapshot: &Snapshot, text: &str) -> Option<TextHit> {
    let needle = text.to_lowercase();
    if snapshot.window.title.to_lowercase().contains(&needle) {
        return Some(TextHit::Title);
    }
    let shown = snapshot.elements.iter().any(|element| element.is_visible() && holds(element, &needle));
    shown.then_some(TextHit::Element)
}

fn holds(element: &Element, needle: &str) -> bool {
    element.name.to_lowercase().contains(needle)
        || element.value.as_deref().is_some_and(|value| value.to_lowercase().contains(needle))
}

#[cfg(test)]
mod tests {
    use winpilot_core::{Rect, Role};

    use super::*;
    use crate::test_fixture::window;

    fn element(id: u32, name: &str, value: Option<&str>, offscreen: bool) -> Element {
        let mut element = crate::test_fixture::element(id, 1, Role::Text, name);
        element.value = value.map(str::to_string);
        element.state.offscreen = offscreen;
        element
    }

    fn snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
        Snapshot {
            window: window(5, "app.exe", title, Rect::new(0, 0, 100, 100)),
            elements,
            focused: None,
            truncated: false,
        }
    }

    #[test]
    fn titles_match_case_insensitively() {
        let snap = snapshot("Upload Complete", Vec::new());
        assert_eq!(find_text(&snap, "upload complete"), Some(TextHit::Title));
        assert!(text_check(&snap, "COMPLETE").met);
    }

    #[test]
    fn names_and_values_are_searched() {
        let snap =
            snapshot("App", vec![element(1, "Status", Some("Saved 3 files"), false), element(2, "Ready", None, false)]);
        assert_eq!(find_text(&snap, "READY"), Some(TextHit::Element));
        let check = text_check(&snap, "3 files");
        assert!(check.met);
        assert_eq!(check.detail, "\"3 files\" is showing");
    }

    #[test]
    fn hidden_elements_do_not_count() {
        let snap = snapshot("App", vec![element(1, "Done", None, true)]);
        assert_eq!(find_text(&snap, "done"), None);
        let check = text_check(&snap, "Done");
        assert!(!check.met);
        assert_eq!(check.detail, "\"Done\" is not showing in w5 app.exe \"App\"");
    }

    #[test]
    fn text_found_by_ocr_meets_the_wait_and_says_how() {
        let shown = super::super::check::Shown::Text { text: "Export done".to_string(), rect: Rect::new(1, 2, 3, 4) };
        let check = read_by_ocr("export done", Presence::certain(Some(shown))).unwrap();
        assert!(check.met);
        assert_eq!(check.detail, "\"export done\" is showing (read by OCR)");
        assert_eq!(read_by_ocr("export done", Presence::certain(None)), None);
    }
}
