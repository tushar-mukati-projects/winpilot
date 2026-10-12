use deckhand_core::{Element, Error, Result};

use super::clip::{NOTE_LIMIT, quoted};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueCheck<'a> {
    Shows(&'a str),
    Differs(&'a str),
    Unreadable,
}

fn value_check<'a>(element: &'a Element, option: &str) -> ValueCheck<'a> {
    match element.value.as_deref() {
        None => ValueCheck::Unreadable,
        Some(value) if shows_option(value, option) => ValueCheck::Shows(value),
        Some(value) => ValueCheck::Differs(value),
    }
}

pub(crate) fn pattern_outcome(element: &Element, option: &str) -> Result<Option<String>> {
    match value_check(element, option) {
        ValueCheck::Shows(value) => Ok(Some(value_note(value))),
        ValueCheck::Differs(value) => Err(Error::unsupported(format!(
            "{} did not take \"{option}\": its value is still {}",
            element.id,
            quoted(value, NOTE_LIMIT)
        ))),
        ValueCheck::Unreadable => Ok(None),
    }
}

pub(crate) fn mouse_outcome(element: &Element, option: &str) -> Result<Option<String>> {
    match value_check(element, option) {
        ValueCheck::Shows(value) => Ok(Some(value_note(value))),
        ValueCheck::Differs(value) => Err(Error::unsupported(format!(
            "{} still shows {} after clicking \"{option}\"",
            element.id,
            quoted(value, NOTE_LIMIT)
        ))),
        ValueCheck::Unreadable if shows_option(&element.name, option) => {
            Ok(Some(format!("now {}", quoted(element.name.trim(), NOTE_LIMIT))))
        }
        ValueCheck::Unreadable => {
            Ok(Some(format!("clicked \"{option}\"; {} does not expose its value to confirm it", element.id)))
        }
    }
}

pub(crate) fn item_outcome(element: &Element) -> Result<Option<String>> {
    if element.state.selected == Some(false) {
        return Err(Error::unsupported(format!("{} is still not selected", element.id)));
    }
    let name = element.name.trim();
    Ok((!name.is_empty()).then(|| format!("selected {}", quoted(name, NOTE_LIMIT))))
}

pub(crate) fn deselected_outcome(element: &Element) -> Result<Option<String>> {
    if element.state.selected == Some(true) {
        return Err(Error::unsupported(format!("{} is still selected", element.id)));
    }
    let name = element.name.trim();
    Ok((!name.is_empty()).then(|| format!("deselected {}", quoted(name, NOTE_LIMIT))))
}

fn shows_option(value: &str, option: &str) -> bool {
    let value = value.trim().to_lowercase();
    let option = option.trim().to_lowercase();
    !value.is_empty() && value.contains(&option)
}

fn value_note(value: &str) -> String {
    format!("value now {}", quoted(value, NOTE_LIMIT))
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, Role};

    use super::*;

    fn element(name: &str, value: Option<&str>, selected: Option<bool>) -> Element {
        let mut element = crate::test_fixture::element(7, 1, Role::Combobox, name);
        element.value = value.map(str::to_string);
        element.state.selected = selected;
        element
    }

    #[test]
    fn values_are_classified() {
        assert_eq!(value_check(&element("Colour", Some("Green"), None), "green"), ValueCheck::Shows("Green"));
        assert_eq!(value_check(&element("Colour", Some(""), None), "Green"), ValueCheck::Differs(""));
        assert_eq!(value_check(&element("Colour", None, None), "Green"), ValueCheck::Unreadable);
    }

    #[test]
    fn a_taken_option_is_noted_from_the_value() {
        let note = pattern_outcome(&element("Colour", Some("Green"), None), "green").unwrap();
        assert_eq!(note.as_deref(), Some("value now \"Green\""));
        let note = mouse_outcome(&element("Colour", Some("Green"), None), "Green").unwrap();
        assert_eq!(note.as_deref(), Some("value now \"Green\""));
    }

    #[test]
    fn an_option_that_did_not_take_is_unsupported() {
        let error = pattern_outcome(&element("Colour", Some(""), None), "Green").unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(error.message, "e7 did not take \"Green\": its value is still \"\"");
        let error = mouse_outcome(&element("Colour", Some("Red"), None), "Green").unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(error.message, "e7 still shows \"Red\" after clicking \"Green\"");
    }

    #[test]
    fn without_a_value_the_pattern_cannot_be_checked() {
        assert_eq!(pattern_outcome(&element("Colours", None, None), "Green").unwrap(), None);
    }

    #[test]
    fn without_a_value_the_mouse_path_falls_back_to_the_name() {
        let named = mouse_outcome(&element("Green", None, None), "green").unwrap();
        assert_eq!(named.as_deref(), Some("now \"Green\""));
        let unconfirmed = mouse_outcome(&element("Colour", None, None), "Green").unwrap();
        assert_eq!(unconfirmed.as_deref(), Some("clicked \"Green\"; e7 does not expose its value to confirm it"));
    }

    #[test]
    fn a_selected_item_is_named() {
        assert_eq!(item_outcome(&element("Item 3", None, Some(true))).unwrap().as_deref(), Some("selected \"Item 3\""));
        assert_eq!(item_outcome(&element(" ", None, None)).unwrap(), None);
    }

    #[test]
    fn a_deselected_item_is_named_and_one_still_selected_is_an_error() {
        let note = deselected_outcome(&element("Item 3", None, Some(false))).unwrap();
        assert_eq!(note.as_deref(), Some("deselected \"Item 3\""));
        let error = deselected_outcome(&element("Item 3", None, Some(true))).unwrap_err();
        assert_eq!(error.message, "e7 is still selected");
    }

    #[test]
    fn an_item_still_unselected_is_an_error() {
        let error = item_outcome(&element("Item 3", None, Some(false))).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
    }
}
