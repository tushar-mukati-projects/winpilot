use deckhand_core::{Element, Error, Result};
use windows::Win32::UI::Accessibility::{IUIAutomationElement, UIA_PATTERN_ID};
use windows_core::Interface;

use crate::{pattern::current_pattern, text::truncate_chars};

const LABEL_NAME_CHARS: usize = 60;

pub(crate) struct Resolved {
    pub(crate) element: IUIAutomationElement,
    pub(crate) info: Element,
}

impl Resolved {
    pub(crate) fn label(&self) -> String {
        describe(&self.info)
    }

    pub(crate) fn doing(&self, verb: &str) -> String {
        format!("{verb} {}", self.label())
    }

    pub(crate) fn pattern<T: Interface>(&self, id: UIA_PATTERN_ID, name: &str) -> Result<T> {
        current_pattern(&self.element, id, &self.doing(&format!("reading the {name} pattern of")))?
            .ok_or_else(|| Error::unsupported(format!("{} has no {name} pattern", self.label())))
    }
}

pub(crate) fn describe(info: &Element) -> String {
    if info.name.is_empty() {
        return format!("{} ({})", info.id, info.role);
    }
    let mut name = truncate_chars(info.name.clone(), LABEL_NAME_CHARS);
    if name.len() < info.name.len() {
        name.push('…');
    }
    format!("{} ({} \"{name}\")", info.id, info.role)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementRef, ElementState, Patterns, Rect, Role};

    use super::*;

    fn info(name: &str) -> Element {
        Element {
            id: ElementRef(12),
            depth: 0,
            role: Role::Button,
            name: name.to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::default(),
            state: ElementState::default(),
            patterns: Patterns::default(),
        }
    }

    #[test]
    fn labels_name_role_and_ref() {
        assert_eq!(describe(&info("Save")), "e12 (button \"Save\")");
        assert_eq!(describe(&info("")), "e12 (button)");
    }

    #[test]
    fn long_names_are_clipped() {
        let label = describe(&info(&"x".repeat(100)));
        assert!(label.ends_with("…\")"));
        assert!(label.len() < 90);
    }
}
