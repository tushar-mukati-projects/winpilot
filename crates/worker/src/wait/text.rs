use deckhand_core::{Element, Result, Snapshot, format::window_line};

use super::check::{Check, unless_gone};
use crate::{engine::Engine, resolve, watch::Watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextHit<'a> {
    Title,
    Element(&'a Element),
}

pub fn check(engine: &Engine, text: &str, scope: Option<&str>) -> Result<Check> {
    let Some(window) = unless_gone(resolve::window(engine, scope))? else {
        return Ok(Check::unmet(format!("no window to search for \"{text}\"")));
    };
    let Some(snapshot) = unless_gone(engine.snapshot(&window))? else {
        return Ok(Check::unmet(format!("{} closed before \"{text}\" showed", window_line(&window))));
    };
    Ok(text_check(&snapshot, text))
}

pub fn watch(engine: &Engine, scope: Option<&str>) -> Watch {
    resolve::window(engine, scope).map(|window| Watch::window(&window)).unwrap_or_default()
}

pub fn text_check(snapshot: &Snapshot, text: &str) -> Check {
    match find_text(snapshot, text) {
        Some(TextHit::Title) => {
            Check::met(format!("the title of {} contains \"{text}\"", window_line(&snapshot.window)))
        }
        Some(TextHit::Element(element)) => {
            Check::met(format!("\"{text}\" is showing")).with_element(Some(element.clone()))
        }
        None => Check::unmet(format!("\"{text}\" is not showing in {}", window_line(&snapshot.window))),
    }
}

pub fn find_text<'a>(snapshot: &'a Snapshot, text: &str) -> Option<TextHit<'a>> {
    let needle = text.to_lowercase();
    if snapshot.window.title.to_lowercase().contains(&needle) {
        return Some(TextHit::Title);
    }
    snapshot.elements.iter().find(|element| element.is_visible() && holds(element, &needle)).map(TextHit::Element)
}

fn holds(element: &Element, needle: &str) -> bool {
    element.name.to_lowercase().contains(needle)
        || element.value.as_deref().is_some_and(|value| value.to_lowercase().contains(needle))
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementRef, ElementState, Patterns, Rect, Role, WindowInfo, WindowRef, WindowState};

    use super::*;

    fn element(id: u32, name: &str, value: Option<&str>, offscreen: bool) -> Element {
        Element {
            id: ElementRef(id),
            depth: 1,
            role: Role::Text,
            name: name.to_string(),
            value: value.map(str::to_string),
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(0, 0, 10, 10),
            state: ElementState { enabled: true, offscreen, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    fn snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
        let window = WindowInfo {
            hwnd: WindowRef(5),
            title: title.to_string(),
            process: "app.exe".to_string(),
            pid: 1,
            class_name: "App".to_string(),
            rect: Rect::new(0, 0, 100, 100),
            state: WindowState::Normal,
            foreground: false,
            elevated: false,
            desktop: None,
            on_current_desktop: true,
        };
        Snapshot { window, elements, focused: None, truncated: false }
    }

    #[test]
    fn titles_match_case_insensitively() {
        let snap = snapshot("Upload Complete", Vec::new());
        assert_eq!(find_text(&snap, "upload complete"), Some(TextHit::Title));
        assert!(text_check(&snap, "COMPLETE").met);
    }

    #[test]
    fn names_and_values_are_searched_in_order() {
        let snap =
            snapshot("App", vec![element(1, "Status", Some("Saved 3 files"), false), element(2, "saved", None, false)]);
        assert_eq!(find_text(&snap, "SAVED").map(hit_id), Some(Some(1)));
        let check = text_check(&snap, "3 files");
        assert!(check.met);
        assert_eq!(check.detail, "\"3 files\" is showing");
        assert_eq!(check.element.map(|found| found.id), Some(ElementRef(1)));
    }

    #[test]
    fn hidden_elements_do_not_count() {
        let snap = snapshot("App", vec![element(1, "Done", None, true)]);
        assert_eq!(find_text(&snap, "done"), None);
        let check = text_check(&snap, "Done");
        assert!(!check.met);
        assert_eq!(check.detail, "\"Done\" is not showing in w5 app.exe \"App\"");
    }

    fn hit_id(hit: TextHit<'_>) -> Option<u32> {
        match hit {
            TextHit::Title => None,
            TextHit::Element(element) => Some(element.id.0),
        }
    }
}
