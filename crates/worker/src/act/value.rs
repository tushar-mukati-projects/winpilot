use deckhand_core::{Element, ElementRef, ErrorCode, Result, TargetSpec, WindowInfo, keys::NamedKey};

use super::{
    chords::{named, with_ctrl},
    clip::{NOTE_LIMIT, quoted},
    done::{Done, via_focus_click},
    focus::focus_prepared,
    landing::ensure_keys_land,
    settle,
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine};

const UNREADABLE: &str = "the field does not expose its value, so it was not read back";

pub fn step(engine: &Engine, target: &TargetSpec, value: &str, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let element = prepared.require_element("set_value")?;
    if is_writable(element)
        && let Some(done) = set_by_pattern(engine, &prepared.window, element.id, value, call)?
    {
        return Ok(done);
    }
    set_by_keys(engine, &prepared, element.id, value, call)
}

pub fn value_matches(now: &str, wanted: &str) -> bool {
    now.trim() == wanted.trim()
}

fn is_writable(element: &Element) -> bool {
    element.patterns.value && !element.state.read_only
}

fn set_by_pattern(
    engine: &Engine,
    window: &WindowInfo,
    id: ElementRef,
    value: &str,
    call: &CallScope,
) -> Result<Option<Done>> {
    let text = value.to_string();
    match engine.uia.run(move |uia| uia.set_value(id, &text)) {
        Ok(()) => {}
        Err(error) if error.code == ErrorCode::Unsupported => return Ok(None),
        Err(error) => return Err(error),
    }
    settle(engine, window, call);
    Ok(match read_value(engine, id)? {
        None => Some(Done::by("value").with_note(Some(UNREADABLE.to_string()))),
        Some(now) if value_matches(&now, value) => Some(Done::by("value")),
        Some(_) => None,
    })
}

fn set_by_keys(engine: &Engine, prepared: &Prepared, id: ElementRef, value: &str, call: &CallScope) -> Result<Done> {
    let focused = focus_prepared(engine, prepared, call)?;
    ensure_keys_land(engine, &focused)?;
    deckhand_input::press_chord(&with_ctrl('a'))?;
    if value.is_empty() {
        deckhand_input::press_chord(&named(NamedKey::Delete))?;
    } else {
        deckhand_input::type_text(value)?;
    }
    settle(engine, &focused.window, call);
    let done = Done::by(via_focus_click("keys", focused.clicked));
    Ok(done.with_note(mismatch_note(read_value(engine, id)?, value)))
}

fn read_value(engine: &Engine, id: ElementRef) -> Result<Option<String>> {
    engine.uia.run(move |uia| uia.value(id))
}

pub fn mismatch_note(now: Option<String>, wanted: &str) -> Option<String> {
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
}
