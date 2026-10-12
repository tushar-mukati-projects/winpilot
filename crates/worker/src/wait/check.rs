use std::time::Instant;

use deckhand_core::{Element, Error, ErrorCode, Rect, Result, Target, WindowInfo, format::window_line, ocr::coords};

use super::WaitReport;
use crate::engine::duration_ms;

const INCOMPLETE: &str = "the window has more elements than one snapshot reads, so its absence is not proven";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub met: bool,
    pub detail: String,
    pub element: Option<Element>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    Element(Element),
    Window(WindowInfo),
    Text { text: String, rect: Rect },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    pub shown: Option<Shown>,
    pub complete: bool,
}

impl Presence {
    pub fn certain(shown: Option<Shown>) -> Self {
        Self { shown, complete: true }
    }
}

impl Check {
    pub fn met(detail: impl Into<String>) -> Self {
        Self { met: true, detail: detail.into(), element: None }
    }

    pub fn unmet(detail: impl Into<String>) -> Self {
        Self { met: false, detail: detail.into(), element: None }
    }

    #[must_use]
    pub fn with_element(self, element: Option<Element>) -> Self {
        Self { element, ..self }
    }

    pub fn report(self, started: Instant) -> WaitReport {
        WaitReport {
            satisfied: self.met,
            elapsed_ms: duration_ms(started.elapsed()),
            detail: self.detail,
            element: self.element,
        }
    }
}

impl Shown {
    fn describe(&self, label: &str) -> String {
        match self {
            Self::Element(_) => label.to_string(),
            Self::Window(window) => format!("{label} ({})", window_line(window)),
            Self::Text { text, rect } => format!("{label} (\"{text}\" {}, read by OCR)", coords(*rect)),
        }
    }

    fn element(self) -> Option<Element> {
        match self {
            Self::Element(element) => Some(element),
            Self::Window(_) | Self::Text { .. } => None,
        }
    }
}

pub fn is_gone(error: &Error) -> bool {
    matches!(error.code, ErrorCode::StaleRef | ErrorCode::NotFound)
}

pub fn unless_gone<T>(result: Result<T>) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if is_gone(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn appears(label: &str, presence: Presence) -> Check {
    match presence.shown {
        Some(shown) => Check::met(format!("{} is visible", shown.describe(label))).with_element(shown.element()),
        None => Check::unmet(format!("{label} has not appeared")),
    }
}

pub fn disappears(label: &str, presence: Presence) -> Check {
    match presence.shown {
        Some(shown) => {
            Check::unmet(format!("{} is still visible", shown.describe(label))).with_element(shown.element())
        }
        None if !presence.complete => Check::unmet(format!("{label} may still be there: {INCOMPLETE}")),
        None => Check::met(format!("{label} is gone")),
    }
}

pub fn target_label(target: &Target) -> String {
    match target {
        Target::Element(id) => id.to_string(),
        Target::Window(hwnd) => hwnd.to_string(),
        Target::Selector(selector) => selector.to_string(),
        Target::Point { point, .. } => format!("{},{}", point.x, point.y),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementRef, ElementState, Patterns, Point, Rect, Role, WindowRef, WindowState};

    use super::*;

    fn element() -> Element {
        Element {
            id: ElementRef(16),
            depth: 1,
            role: Role::Button,
            name: "Appeared".to_string(),
            value: None,
            automation_id: "2016".to_string(),
            class_name: "Button".to_string(),
            rect: Rect::new(0, 0, 80, 24),
            state: ElementState { enabled: true, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    fn window() -> WindowInfo {
        WindowInfo {
            hwnd: WindowRef(77),
            title: "Save As".to_string(),
            process: "notepad.exe".to_string(),
            pid: 1,
            class_name: "#32770".to_string(),
            rect: Rect::new(0, 0, 400, 300),
            state: WindowState::Normal,
            foreground: false,
            elevated: false,
            desktop: None,
            on_current_desktop: true,
        }
    }

    #[test]
    fn an_appearing_element_is_reported_with_it() {
        let check = appears("id:2016", Presence::certain(Some(Shown::Element(element()))));
        assert!(check.met);
        assert_eq!(check.detail, "id:2016 is visible");
        assert_eq!(check.element, Some(element()));
        let missing = appears("id:2016", Presence::certain(None));
        assert!(!missing.met);
        assert_eq!(missing.detail, "id:2016 has not appeared");
        assert_eq!(missing.element, None);
    }

    #[test]
    fn disappearing_is_the_negation() {
        let still = disappears("e16", Presence::certain(Some(Shown::Element(element()))));
        assert!(!still.met);
        assert_eq!(still.detail, "e16 is still visible");
        assert_eq!(still.element, Some(element()));
        let gone = disappears("e16", Presence::certain(None));
        assert!(gone.met);
        assert_eq!(gone.detail, "e16 is gone");
    }

    #[test]
    fn a_truncated_snapshot_does_not_prove_absence() {
        let unsure = Presence { shown: None, complete: false };
        let check = disappears("name:Spinner", unsure.clone());
        assert!(!check.met);
        assert!(check.detail.starts_with("name:Spinner may still be there"), "{}", check.detail);
        assert!(!appears("name:Spinner", unsure).met);
    }

    #[test]
    fn windows_are_described_by_their_line() {
        let check = appears("w77", Presence::certain(Some(Shown::Window(window()))));
        assert!(check.met);
        assert_eq!(check.detail, "w77 (w77 notepad.exe \"Save As\") is visible");
        assert_eq!(check.element, None);
    }

    #[test]
    fn text_read_by_ocr_is_described_with_its_position() {
        let shown = Shown::Text { text: "Export".to_string(), rect: Rect::new(40, 50, 60, 18) };
        let check = appears("ocr:Export", Presence::certain(Some(shown.clone())));
        assert!(check.met);
        assert_eq!(check.detail, "ocr:Export (\"Export\" @40,50,60x18, read by OCR) is visible");
        assert_eq!(check.element, None);
        let still = disappears("ocr:Export", Presence::certain(Some(shown)));
        assert!(!still.met);
        assert!(still.detail.ends_with("is still visible"), "{}", still.detail);
    }

    #[test]
    fn stale_and_missing_count_as_gone() {
        assert!(is_gone(&Error::new(ErrorCode::StaleRef, "e3 is gone")));
        assert!(is_gone(&Error::not_found("no window")));
        assert!(!is_gone(&Error::new(ErrorCode::Denied, "denied")));
        assert_eq!(unless_gone::<u8>(Err(Error::not_found("x"))).unwrap(), None);
        assert_eq!(unless_gone(Ok(3)).unwrap(), Some(3));
        assert!(unless_gone::<u8>(Err(Error::new(ErrorCode::ElevatedTarget, "admin"))).is_err());
    }

    #[test]
    fn labels_name_each_kind_of_target() {
        assert_eq!(target_label(&Target::Element(ElementRef(4))), "e4");
        assert_eq!(target_label(&Target::Window(WindowRef(9))), "w9");
        assert_eq!(target_label(&Target::Point { point: Point::new(3, 4), image: None }), "3,4");
        let selector = "role:button name:OK".parse().unwrap();
        assert!(target_label(&Target::Selector(selector)).contains("OK"));
    }

    #[test]
    fn reports_carry_the_verdict() {
        let report = Check::met("done").with_element(Some(element())).report(Instant::now());
        assert!(report.satisfied);
        assert_eq!(report.detail, "done");
        assert_eq!(report.element, Some(element()));
        assert!(!Check::unmet("no").report(Instant::now()).satisfied);
    }
}
