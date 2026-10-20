use std::time::Duration;

use winpilot_core::{
    Element, ElementRef, Result, WindowInfo,
    keys::{Chord, Key, Modifiers, NamedKey},
};
use winpilot_uia::Uia;

use super::{
    field::{is_writable, read_value},
    focus::Focused,
    paste::{Readback, readback},
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const PACING_WAIT: Duration = Duration::from_millis(500);
const TYPED_TAIL: usize = 32;
const CTRL_EDITS: [char; 4] = ['v', 'x', 'z', 'y'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unreadable {
    EndsWait,
    KeepWaiting,
}

pub(crate) fn edits_text(chords: &[Chord]) -> bool {
    chords.iter().any(edits)
}

pub(crate) fn editable_field(engine: &Engine, element: Option<&Element>) -> Option<Readback> {
    readback(engine, element.filter(|element| is_writable(element)))
}

pub(crate) fn focused_field(engine: &Engine, focused: &Focused) -> Option<Readback> {
    editable_field(engine, focused.element.as_ref())
        .or_else(|| editable_field(engine, engine.uia.run(Uia::focused).ok().flatten().as_ref()))
}

pub(crate) fn wait_for_edit(
    engine: &Engine,
    window: &WindowInfo,
    call: &CallScope,
    field: Option<&Readback>,
    typed: Option<&str>,
) -> Result<()> {
    let Some(field) = field else { return Ok(()) };
    let limit = call.bounded(PACING_WAIT);
    let landed = |now: &str| typed.map_or(now != field.before, |text| typed_landed(&field.before, now, text));
    wait_for_value(engine, window, call, field.id, limit, Unreadable::EndsWait, landed).map(drop)
}

pub(crate) fn wait_for_value(
    engine: &Engine,
    window: &WindowInfo,
    call: &CallScope,
    id: ElementRef,
    limit: Duration,
    unreadable: Unreadable,
    landed: impl Fn(&str) -> bool,
) -> Result<bool> {
    let found = until(engine, call, limit, &Watch::window(window), || {
        Ok(verdict(read_value(engine, id), unreadable, &landed))
    })?;
    Ok(found == Some(true))
}

fn verdict(read: Result<Option<String>>, unreadable: Unreadable, landed: impl Fn(&str) -> bool) -> Option<bool> {
    match (read, unreadable) {
        (Ok(now), _) => now.filter(|now| landed(now)).map(|_| true),
        (Err(_), Unreadable::EndsWait) => Some(false),
        (Err(_), Unreadable::KeepWaiting) => None,
    }
}

fn edits(chord: &Chord) -> bool {
    let Modifiers { ctrl, alt, shift, win } = chord.modifiers;
    if alt || win {
        return false;
    }
    match chord.key {
        Some(Key::Named(NamedKey::Backspace | NamedKey::Delete)) => true,
        Some(Key::Named(NamedKey::Insert)) => shift && !ctrl,
        Some(Key::Named(NamedKey::Space)) => !ctrl,
        Some(Key::Char(key)) => !ctrl || CTRL_EDITS.contains(&key),
        Some(Key::Named(_) | Key::Function(_)) | None => false,
    }
}

fn typed_landed(before: &str, now: &str, text: &str) -> bool {
    text.is_empty() || (now != before && now.contains(typed_tail(text)))
}

fn typed_tail(text: &str) -> &str {
    let trimmed = text.trim();
    let line = trimmed.rsplit_once('\n').map_or(trimmed, |(_, last)| last);
    line.char_indices().rev().nth(TYPED_TAIL - 1).map_or(line, |(at, _)| &line[at..])
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use winpilot_core::{Rect, keys::parse_keys};
    use winpilot_win32::Cancel;

    use super::*;
    use crate::test_fixture::{engine, window};

    #[test]
    fn only_keys_that_edit_text_wait_for_the_value() {
        let cases = [
            ("backspace", true),
            ("delete", true),
            ("ctrl+backspace", true),
            ("ctrl+delete", true),
            ("shift+delete", true),
            ("shift+insert", true),
            ("ctrl+v", true),
            ("ctrl+x", true),
            ("ctrl+z", true),
            ("ctrl+y", true),
            ("a", true),
            ("shift+a", true),
            ("space", true),
            ("ctrl+a backspace", true),
            ("tab", false),
            ("enter", false),
            ("escape", false),
            ("left", false),
            ("home", false),
            ("pagedown", false),
            ("insert", false),
            ("ctrl+s", false),
            ("ctrl+a", false),
            ("alt+f4", false),
            ("alt+backspace", false),
            ("win+d", false),
            ("f5", false),
            ("shift", false),
        ];
        for (keys, waits) in cases {
            assert_eq!(edits_text(&parse_keys(keys).unwrap()), waits, "keys: {keys}");
        }
    }

    #[test]
    fn typed_text_has_landed_once_the_field_shows_it() {
        assert!(typed_landed("", "Microsoft Azure", "Microsoft Azure"));
        assert!(!typed_landed("", "Microsoft Azur", "Microsoft Azure"));
        assert!(typed_landed("Microsoft Azur", "Microsoft Azure", "e"));
        assert!(!typed_landed("hello", "hello", "o"));
        assert!(typed_landed("hello", "hello", ""));
        assert!(typed_landed("", "first\r\nsecond", "first\nsecond\n"));
    }

    fn wait_on_unreadable(unreadable: Unreadable) -> (bool, Duration) {
        let window = window(1, "app.exe", "App", Rect::new(0, 0, 10, 10));
        let call = CallScope::new(Cancel::default(), Instant::now() + PACING_WAIT * 4);
        let started = Instant::now();
        let landed = wait_for_value(engine(), &window, &call, ElementRef(999_999), PACING_WAIT, unreadable, |_| true);
        (landed.unwrap(), started.elapsed())
    }

    #[test]
    fn a_value_that_cannot_be_read_ends_the_key_wait_at_once() {
        let (landed, elapsed) = wait_on_unreadable(Unreadable::EndsWait);
        assert!(!landed);
        assert!(elapsed < PACING_WAIT, "{elapsed:?}");
    }

    #[test]
    fn a_paste_keeps_waiting_through_failed_reads() {
        let (landed, elapsed) = wait_on_unreadable(Unreadable::KeepWaiting);
        assert!(!landed);
        assert!(elapsed >= PACING_WAIT, "{elapsed:?}");
    }

    #[test]
    fn a_failed_read_ends_the_wait_or_counts_as_not_yet() {
        let failed = || Err(winpilot_core::Error::unsupported("timed out"));
        assert_eq!(verdict(failed(), Unreadable::EndsWait, |_| true), Some(false));
        assert_eq!(verdict(failed(), Unreadable::KeepWaiting, |_| true), None);
        for unreadable in [Unreadable::EndsWait, Unreadable::KeepWaiting] {
            assert_eq!(verdict(Ok(Some("new".to_string())), unreadable, |now| now == "new"), Some(true));
            assert_eq!(verdict(Ok(Some("old".to_string())), unreadable, |now| now == "new"), None);
            assert_eq!(verdict(Ok(None), unreadable, |_| true), None);
        }
    }

    #[test]
    fn long_text_is_matched_by_its_last_characters() {
        let long = format!("{}{}", "x".repeat(50), "é".repeat(TYPED_TAIL));
        assert_eq!(typed_tail(&long), "é".repeat(TYPED_TAIL));
        assert_eq!(typed_tail("  short  "), "short");
        assert_eq!(typed_tail(""), "");
    }
}
