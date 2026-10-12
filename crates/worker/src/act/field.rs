use deckhand_core::{Element, ElementRef, Result};

use crate::engine::Engine;

pub(crate) fn read_value(engine: &Engine, id: ElementRef) -> Result<Option<String>> {
    engine.uia.run(move |uia| uia.value(id))
}

pub(crate) fn is_writable(element: &Element) -> bool {
    element.patterns.value && !element.state.read_only
}

pub(crate) fn is_readable(element: &Element) -> bool {
    element.patterns.value && !element.state.password
}

#[cfg(test)]
mod tests {
    use deckhand_core::Role;

    use super::*;
    use crate::test_fixture::element;

    fn edit(value: bool, read_only: bool, password: bool) -> Element {
        let mut edit = element(1, 1, Role::Edit, "Name");
        edit.patterns.value = value;
        edit.state.read_only = read_only;
        edit.state.password = password;
        edit
    }

    #[test]
    fn a_field_is_writable_through_an_editable_value_pattern() {
        assert!(is_writable(&edit(true, false, false)));
        assert!(is_writable(&edit(true, false, true)));
        assert!(!is_writable(&edit(true, true, false)));
        assert!(!is_writable(&edit(false, false, false)));
    }

    #[test]
    fn a_field_is_readable_unless_it_hides_a_password() {
        assert!(is_readable(&edit(true, true, false)));
        assert!(!is_readable(&edit(true, false, true)));
        assert!(!is_readable(&edit(false, false, false)));
    }
}
