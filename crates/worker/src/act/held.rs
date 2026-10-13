use winpilot_core::{Point, keys::Chord, protocol::StepOutcome};

use super::{key_labels::chord_text, steps::append_note};
use crate::call_scope::{CallScope, HeldButton};

const AT_PRESS_POINT: &str = ", back where it was pressed because the call ended early";

pub(crate) fn release_held(call: &CallScope, outcomes: &mut [StepOutcome]) {
    let ended_early = outcomes.last().is_some_and(|outcome| outcome.error.is_some());
    let buttons = release_buttons(&call.take_held(), ended_early);
    let keys = release_keys(&call.take_held_keys());
    let Some(last) = outcomes.last_mut() else { return };
    for note in [buttons, keys].into_iter().flatten() {
        append_note(last, note);
    }
}

pub(crate) fn release_point(held: &HeldButton, ended_early: bool, here: Option<Point>) -> Option<Point> {
    if ended_early { Some(held.pressed_at) } else { here }
}

fn release_buttons(held: &[HeldButton], ended_early: bool) -> Option<String> {
    if held.is_empty() {
        return None;
    }
    let here = winpilot_input::cursor_position().ok();
    let released: Vec<&str> = held
        .iter()
        .filter(|held| {
            let point = release_point(held, ended_early, here);
            point.is_some_and(|point| winpilot_input::button_up(point, held.button).is_ok())
        })
        .map(|held| held.button.name())
        .collect();
    Some(buttons_note(&released, held.len(), ended_early))
}

fn release_keys(held: &[Chord]) -> Option<String> {
    if held.is_empty() {
        return None;
    }
    let released: Vec<String> =
        held.iter().rev().filter(|chord| winpilot_input::release_chord(chord).is_ok()).map(chord_text).collect();
    Some(keys_note(&released, held.len()))
}

fn keys_note(released: &[String], held: usize) -> String {
    if released.len() == held {
        return format!("released {} left held by key down", released.join(" and "));
    }
    "a key left held by key down could not be released; press it once to reset".to_string()
}

fn buttons_note(released: &[&str], held: usize, ended_early: bool) -> String {
    if released.len() != held {
        return "a mouse button left held by mouse down could not be released; press it once to reset".to_string();
    }
    let place = if ended_early { AT_PRESS_POINT } else { "" };
    format!("released the {} mouse button left held by mouse down{place}", released.join(" and "))
}

#[cfg(test)]
mod tests {
    use winpilot_core::protocol::MouseButton;

    use super::*;

    #[test]
    fn the_release_notes_name_what_was_held() {
        assert_eq!(buttons_note(&["left"], 1, false), "released the left mouse button left held by mouse down");
        assert_eq!(
            buttons_note(&["left", "right"], 2, false),
            "released the left and right mouse button left held by mouse down"
        );
        assert_eq!(
            buttons_note(&["left"], 1, true),
            "released the left mouse button left held by mouse down, back where it was pressed because the call ended early"
        );
        assert!(buttons_note(&[], 1, false).contains("could not be released"));
        assert_eq!(keys_note(&["shift".to_string()], 1), "released shift left held by key down");
        assert!(keys_note(&[], 2).contains("could not be released"));
    }

    #[test]
    fn a_call_that_ended_early_drops_back_at_the_press_point() {
        let held = HeldButton { button: MouseButton::Left, pressed_at: Point::new(10, 20) };
        let here = Some(Point::new(300, 400));
        assert_eq!(release_point(&held, true, here), Some(Point::new(10, 20)));
        assert_eq!(release_point(&held, false, here), here);
        assert_eq!(release_point(&held, false, None), None);
    }

    #[test]
    fn nothing_held_means_no_note() {
        let call = CallScope::relaxed();
        let mut outcomes = Vec::new();
        release_held(&call, &mut outcomes);
        assert!(outcomes.is_empty());
        assert_eq!(release_keys(&[]), None);
        assert_eq!(release_buttons(&[], true), None);
    }
}
