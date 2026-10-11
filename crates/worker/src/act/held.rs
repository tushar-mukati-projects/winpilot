use deckhand_core::{
    keys::Chord,
    protocol::{MouseButton, StepOutcome},
};

use super::{key_labels::chord_text, steps::append_note};
use crate::call_scope::CallScope;

pub fn release_held(call: &CallScope, outcomes: &mut [StepOutcome]) {
    let notes: Vec<String> =
        [release_keys(&call.take_held_keys()), release_buttons(&call.take_held())].into_iter().flatten().collect();
    let Some(last) = outcomes.last_mut() else { return };
    for note in notes {
        append_note(last, note);
    }
}

fn release_keys(held: &[Chord]) -> Option<String> {
    if held.is_empty() {
        return None;
    }
    let released: Vec<String> =
        held.iter().rev().filter(|chord| deckhand_input::release_chord(chord).is_ok()).map(chord_text).collect();
    Some(keys_note(&released, held.len()))
}

fn release_buttons(held: &[MouseButton]) -> Option<String> {
    if held.is_empty() {
        return None;
    }
    let point = deckhand_input::cursor_position();
    let released: Vec<&str> = held
        .iter()
        .filter(|button| point.as_ref().is_ok_and(|point| deckhand_input::button_up(*point, **button).is_ok()))
        .map(|button| button.name())
        .collect();
    Some(buttons_note(&released, held.len()))
}

fn keys_note(released: &[String], held: usize) -> String {
    if released.len() == held {
        return format!("released {} left held by key down", released.join(" and "));
    }
    "a key left held by key down could not be released; press it once to reset".to_string()
}

fn buttons_note(released: &[&str], held: usize) -> String {
    if released.len() == held {
        return format!("released the {} mouse button left held by mouse down", released.join(" and "));
    }
    "a mouse button left held by mouse down could not be released; press it once to reset".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_notes_name_what_was_held() {
        assert_eq!(buttons_note(&["left"], 1), "released the left mouse button left held by mouse down");
        assert_eq!(
            buttons_note(&["left", "right"], 2),
            "released the left and right mouse button left held by mouse down"
        );
        assert!(buttons_note(&[], 1).contains("could not be released"));
        assert_eq!(keys_note(&["shift".to_string()], 1), "released shift left held by key down");
        assert!(keys_note(&[], 2).contains("could not be released"));
    }

    #[test]
    fn nothing_held_means_no_note() {
        let call = CallScope::relaxed();
        let mut outcomes = Vec::new();
        release_held(&call, &mut outcomes);
        assert!(outcomes.is_empty());
        assert_eq!(release_keys(&[]), None);
        assert_eq!(release_buttons(&[]), None);
    }
}
