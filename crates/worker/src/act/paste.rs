use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Element, ElementRef, Error, Result, WindowInfo};
use deckhand_desktop::clipboard;

use super::{chords::with_ctrl, settle};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const MIN_PASTE_WAIT: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restore {
    Text(String),
    Clear,
}

pub fn restore_plan(saved: Option<String>) -> Restore {
    saved.map_or(Restore::Clear, Restore::Text)
}

pub fn paste(
    engine: &Engine,
    window: &WindowInfo,
    element: Option<&Element>,
    text: &str,
    call: &CallScope,
) -> Result<Option<String>> {
    let plan = restore_plan(clipboard::get_text()?);
    let pasted = put_and_paste(engine, window, element, text, call);
    let restore_note = restore_with_retry(&plan);
    pasted.map(|()| restore_note)
}

pub fn paste_landed(before: &str, now: &str, text: &str) -> bool {
    now != before || now.contains(text.trim())
}

fn put_and_paste(
    engine: &Engine,
    window: &WindowInfo,
    element: Option<&Element>,
    text: &str,
    call: &CallScope,
) -> Result<()> {
    let readable = element.filter(|element| element.patterns.value && !element.state.password);
    let before = readable.and_then(|element| read_value(engine, element.id).map(|value| (element.id, value)));
    clipboard::set_text_transient(text)?;
    let pressed = Instant::now();
    deckhand_input::press_chord(&with_ctrl('v'))?;
    if let Some((id, before)) = before {
        return await_value_change(engine, window, id, &before, text, call);
    }
    settle(engine, window, call);
    thread::sleep(call.bounded(MIN_PASTE_WAIT.saturating_sub(pressed.elapsed())));
    Ok(())
}

fn await_value_change(
    engine: &Engine,
    window: &WindowInfo,
    id: ElementRef,
    before: &str,
    text: &str,
    call: &CallScope,
) -> Result<()> {
    let limit = Duration::from_millis(engine.config.act.settle_max_ms);
    let landed = until(engine, call, limit, &Watch::window(window), || {
        Ok(read_value(engine, id).filter(|now| paste_landed(before, now, text)).map(drop))
    })?;
    landed.ok_or_else(|| Error::unsupported(format!("pasting did not change the value of {id}; try how=keys")))
}

fn read_value(engine: &Engine, id: ElementRef) -> Option<String> {
    engine.uia.run(move |uia| uia.value(id)).ok().flatten()
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
    use super::*;

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
}
