use deckhand_core::{Error, Result};

use super::{
    by_pattern::by_pattern,
    done::Done,
    field::{is_writable, read_value},
    target::Prepared,
    value::mismatch_note,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const APPENDED: &str = "appended to the existing text; background mode can't place a caret";
const REPLACED: &str = "the field hides its value, so it now holds only the typed text";

pub(crate) fn step(engine: &Engine, prepared: &Prepared, text: &str, call: &CallScope) -> Result<Done> {
    let element = prepared.require_element("type in background mode")?;
    if !is_writable(element) {
        return Err(Error::unsupported(format!(
            "{} has no writable value pattern, so background mode can't type into it",
            element.id
        )));
    }
    if prepared.window.on_current_desktop {
        overlay::focus_field(engine, element.rect);
        overlay::typing(engine, &prepared.window);
    }
    let id = element.id;
    let (value, note) = appended(read_value(engine, id)?.as_deref(), text);
    let wanted = value.clone();
    let ((), handed_back) =
        by_pattern(engine, &prepared.window, call, || engine.uia.run(move |uia| uia.set_value(id, &wanted)))?;
    let check = mismatch_note(read_value(engine, id)?, &value);
    Ok(Done::by("value").with_note(note.map(str::to_string)).with_note(check).with_note(handed_back))
}

pub(crate) fn appended(current: Option<&str>, text: &str) -> (String, Option<&'static str>) {
    match current {
        None => (text.to_string(), Some(REPLACED)),
        Some("") => (text.to_string(), None),
        Some(current) => (format!("{current}{text}"), Some(APPENDED)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_appended_to_what_the_field_holds() {
        assert_eq!(appended(Some("Hello"), ", world"), ("Hello, world".to_string(), Some(APPENDED)));
    }

    #[test]
    fn an_empty_field_simply_takes_the_text() {
        assert_eq!(appended(Some(""), "hi"), ("hi".to_string(), None));
    }

    #[test]
    fn a_hidden_value_is_replaced_and_says_so() {
        assert_eq!(appended(None, "secret"), ("secret".to_string(), Some(REPLACED)));
    }
}
