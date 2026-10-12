use std::time::Duration;

use deckhand_core::{Element, ElementRef, Error, Result, WindowInfo};
use deckhand_desktop::clipboard;

use super::{
    chords::with_ctrl,
    field::{is_readable, read_value},
    lift::with_held_modifiers_lifted,
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const CHANGED_DURING_PASTE: &str =
    "something else changed the clipboard during the paste, so Deckhand left the new contents alone";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Restore {
    Text(String),
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Readback {
    pub(crate) id: ElementRef,
    pub(crate) before: String,
}

fn restore_plan(saved: Option<String>) -> Restore {
    saved.map_or(Restore::Clear, Restore::Text)
}

pub(crate) fn readback(engine: &Engine, element: Option<&Element>) -> Option<Readback> {
    let field = element.filter(|element| is_readable(element))?;
    let before = read_value(engine, field.id).ok().flatten()?;
    Some(Readback { id: field.id, before })
}

pub(crate) fn paste(
    engine: &Engine,
    window: &WindowInfo,
    readback: &Readback,
    text: &str,
    call: &CallScope,
) -> Result<Option<String>> {
    let plan = restore_plan(clipboard::get_text()?);
    let placed = clipboard::set_text_transient(text);
    let ours = clipboard::sequence_number();
    let pasted = placed.and_then(|()| press_and_confirm(engine, window, readback, text, call));
    let restore_note = restore_unless_replaced(&plan, ours, clipboard::sequence_number());
    pasted.map(|()| restore_note)
}

fn paste_landed(before: &str, now: &str, text: &str) -> bool {
    now != before || now.contains(text.trim())
}

fn press_and_confirm(
    engine: &Engine,
    window: &WindowInfo,
    readback: &Readback,
    text: &str,
    call: &CallScope,
) -> Result<()> {
    with_held_modifiers_lifted(call, || deckhand_input::press_chord(&with_ctrl('v')))?;
    let limit = Duration::from_millis(engine.config.act.settle_max_ms);
    let landed = until(engine, call, limit, &Watch::window(window), || {
        let now = read_value(engine, readback.id).ok().flatten();
        Ok(now.filter(|now| paste_landed(&readback.before, now, text)).map(drop))
    })?;
    landed
        .ok_or_else(|| Error::unsupported(format!("pasting did not change the value of {}; try how=keys", readback.id)))
}

fn restore_unless_replaced(plan: &Restore, ours: Option<u32>, now: Option<u32>) -> Option<String> {
    if now != ours {
        return Some(CHANGED_DURING_PASTE.to_string());
    }
    restore_with_retry(plan)
}

fn restore_with_retry(plan: &Restore) -> Option<String> {
    match restore(plan).or_else(|_| restore(plan)) {
        Ok(()) => None,
        Err(error) => Some(format!("the text was pasted but the clipboard could not be restored: {error}")),
    }
}

fn restore(plan: &Restore) -> Result<()> {
    match plan {
        Restore::Text(text) => clipboard::set_text_transient(text),
        Restore::Clear => clipboard::clear(),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::Role;

    use super::*;
    use crate::test_fixture::{element, engine};

    #[test]
    fn saved_text_is_put_back() {
        assert_eq!(restore_plan(Some("kept".to_string())), Restore::Text("kept".to_string()));
        assert_eq!(restore_plan(Some(String::new())), Restore::Text(String::new()));
    }

    #[test]
    fn an_empty_clipboard_is_cleared_again() {
        assert_eq!(restore_plan(None), Restore::Clear);
    }

    #[test]
    fn a_paste_has_landed_once_the_value_changes_or_holds_the_text() {
        assert!(paste_landed("old", "old and new", "and new"));
        assert!(paste_landed("same text", "same text", "same text"));
        assert!(!paste_landed("old", "old", "new"));
    }

    #[test]
    fn a_clipboard_someone_else_changed_is_never_overwritten() {
        let note = restore_unless_replaced(&Restore::Text("kept".to_string()), Some(7), Some(9));
        assert_eq!(note.as_deref(), Some(CHANGED_DURING_PASTE));
        assert_eq!(restore_unless_replaced(&Restore::Clear, None, Some(1)).as_deref(), Some(CHANGED_DURING_PASTE));
    }

    #[test]
    fn a_field_without_a_readable_value_cannot_confirm_a_paste() {
        let mut secret = element(1, 1, Role::Edit, "Password");
        secret.patterns.value = true;
        secret.state.password = true;
        assert_eq!(readback(engine(), Some(&secret)), None);
        assert_eq!(readback(engine(), None), None);
    }
}
