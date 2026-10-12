use deckhand_core::{Element, ElementRef, Error, ErrorCode, Result, WindowInfo, keys::NamedKey};

use super::{
    by_pattern::by_pattern,
    chords::{named, with_ctrl},
    clip::{NOTE_LIMIT, quoted},
    done::{Done, via_focus_click},
    field::{is_writable, read_value},
    focus::keyboard_target,
    lift::with_held_modifiers_lifted,
    mode::Mode,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const UNREADABLE: &str = "the field does not expose its value, so it was not read back";

pub(crate) fn step(engine: &Engine, prepared: &Prepared, value: &str, mode: Mode, call: &CallScope) -> Result<Done> {
    let element = prepared.require_element("set value")?;
    if prepared.window.on_current_desktop {
        overlay::focus_field(engine, element.rect);
        overlay::typing(engine, &prepared.window);
    }
    if mode == Mode::Background {
        return set_in_background(engine, prepared, element, value, call);
    }
    if is_writable(element)
        && let Some(done) = set_by_pattern(engine, &prepared.window, element.id, value, call)?
    {
        return Ok(done);
    }
    set_by_keys(engine, prepared, element.id, value, call)
}

fn set_in_background(
    engine: &Engine,
    prepared: &Prepared,
    element: &Element,
    value: &str,
    call: &CallScope,
) -> Result<Done> {
    if !is_writable(element) {
        return Err(Error::unsupported(format!(
            "{} has no writable value pattern, so background mode can't set it",
            element.id
        )));
    }
    let (id, text) = (element.id, value.to_string());
    let ((), handed_back) =
        by_pattern(engine, &prepared.window, call, || engine.uia.run(move |uia| uia.set_value(id, &text)))?;
    Ok(Done::by("value").with_note(mismatch_note(read_value(engine, id)?, value)).with_note(handed_back))
}

fn value_matches(now: &str, wanted: &str) -> bool {
    now.trim() == wanted.trim()
}

fn left_unchanged(before: Option<&str>, now: Option<&str>, wanted: &str) -> bool {
    now.is_some_and(|now| !value_matches(now, wanted) && before == Some(now))
}

fn set_by_pattern(
    engine: &Engine,
    window: &WindowInfo,
    id: ElementRef,
    value: &str,
    call: &CallScope,
) -> Result<Option<Done>> {
    let before = read_value(engine, id)?;
    let text = value.to_string();
    let handed_back = match by_pattern(engine, window, call, || engine.uia.run(move |uia| uia.set_value(id, &text))) {
        Ok(((), note)) => note,
        Err(error) if error.code == ErrorCode::Unsupported => return Ok(None),
        Err(error) => return Err(error),
    };
    let now = read_value(engine, id)?;
    if left_unchanged(before.as_deref(), now.as_deref(), value) {
        return Ok(None);
    }
    Ok(Some(Done::by("value").with_note(mismatch_note(now, value)).with_note(handed_back)))
}

fn set_by_keys(engine: &Engine, prepared: &Prepared, id: ElementRef, value: &str, call: &CallScope) -> Result<Done> {
    let focused = keyboard_target(engine, Some(prepared), call)?;
    with_held_modifiers_lifted(call, || replace_text(value))?;
    settle(engine, &focused.window, call);
    let done = Done::by(via_focus_click("keys", focused.clicked));
    Ok(done.with_note(mismatch_note(read_value(engine, id)?, value)))
}

fn replace_text(value: &str) -> Result<()> {
    deckhand_input::press_chord(&with_ctrl('a'))?;
    if value.is_empty() {
        return deckhand_input::press_chord(&named(NamedKey::Delete));
    }
    deckhand_input::type_text(value)
}

pub(crate) fn mismatch_note(now: Option<String>, wanted: &str) -> Option<String> {
    match now {
        None => Some(UNREADABLE.to_string()),
        Some(now) if value_matches(&now, wanted) => None,
        Some(now) => Some(format!("value is now {}", quoted(&now, NOTE_LIMIT))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_match_ignoring_surrounding_whitespace() {
        assert!(value_matches("hello", "hello"));
        assert!(value_matches(" hello\n", "hello"));
        assert!(!value_matches("Hello", "hello"));
        assert!(!value_matches("hello world", "hello"));
    }

    #[test]
    fn a_formatted_value_is_noted_not_failed() {
        assert_eq!(mismatch_note(Some("1,000".to_string()), "1000").as_deref(), Some("value is now \"1,000\""));
        assert_eq!(mismatch_note(Some("1000 ".to_string()), "1000"), None);
        assert_eq!(mismatch_note(None, "secret").as_deref(), Some(UNREADABLE));
    }

    #[test]
    fn only_a_write_that_changed_nothing_is_retyped() {
        assert!(left_unchanged(Some("old"), Some("old"), "new"));
        assert!(!left_unchanged(Some(""), Some("1,000"), "1000"));
        assert!(!left_unchanged(Some("1000"), Some("1000"), "1000"));
        assert!(!left_unchanged(None, Some("old"), "new"));
        assert!(!left_unchanged(Some("old"), None, "new"));
    }
}
