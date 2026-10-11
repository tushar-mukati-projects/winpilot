use deckhand_core::{
    Result, TargetSpec,
    protocol::{MouseButton, StepOutcome},
    target::PointSpec,
};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate, locate_in_front},
    motion::move_pointer,
    steps::append_note,
    target::prepare,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

pub struct Press<'a> {
    pub target: Option<&'a TargetSpec>,
    pub button: MouseButton,
    pub glide_ms: Option<u64>,
}

pub fn move_step(engine: &Engine, to: &TargetSpec, glide_ms: Option<u64>, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, to, call)?;
    let located = locate_in_front(engine, &prepared, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    overlay::begin(engine, &located.window, call);
    move_pointer(engine, point, glide_ms)?;
    Ok(Done::by("mouse"))
}

pub fn mouse_down(engine: &Engine, press: &Press<'_>, call: &CallScope) -> Result<Done> {
    let Some(target) = press.target else { return Err(deckhand_core::Error::invalid("mouse_down needs \"target\"")) };
    let prepared = prepare(engine, target, call)?;
    let located = locate_in_front(engine, &prepared, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    overlay::begin(engine, &located.window, call);
    move_pointer(engine, point, press.glide_ms)?;
    deckhand_input::button_down(point, press.button)?;
    call.note_held(press.button);
    overlay::click(engine, point, 1);
    Ok(Done::by("mouse"))
}

pub fn mouse_up(engine: &Engine, press: &Press<'_>, call: &CallScope) -> Result<Done> {
    let here = deckhand_input::cursor_position()?;
    let spot = TargetSpec::Point(PointSpec { x: here.x, y: here.y, img: None });
    let prepared = prepare(engine, press.target.unwrap_or(&spot), call)?;
    let window = deckhand_desktop::window(prepared.window.hwnd)?;
    let located = locate(engine, &prepared, window, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    if point != here {
        move_pointer(engine, point, press.glide_ms)?;
    }
    deckhand_input::button_up(point, press.button)?;
    call.note_released(press.button);
    overlay::click(engine, point, 1);
    Ok(Done::by("mouse"))
}

pub fn release_held(call: &CallScope, outcomes: &mut [StepOutcome]) {
    let held = call.take_held();
    if held.is_empty() {
        return;
    }
    let point = deckhand_input::cursor_position();
    let released: Vec<&str> = held
        .iter()
        .filter(|button| point.as_ref().is_ok_and(|point| deckhand_input::button_up(*point, **button).is_ok()))
        .map(|button| button_name(*button))
        .collect();
    if let Some(last) = outcomes.last_mut() {
        append_note(last, held_note(&released, held.len()));
    }
}

fn held_note(released: &[&str], held: usize) -> String {
    if released.len() == held {
        return format!("released the {} mouse button left held by mouse_down", released.join(" and "));
    }
    "a mouse button left held by mouse_down could not be released; press it once to reset".to_string()
}

const fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_note_names_the_buttons() {
        assert_eq!(held_note(&["left"], 1), "released the left mouse button left held by mouse_down");
        assert_eq!(
            held_note(&["left", "right"], 2),
            "released the left and right mouse button left held by mouse_down"
        );
        assert!(held_note(&[], 1).contains("could not be released"));
    }

    #[test]
    fn nothing_held_means_no_note() {
        let call = CallScope::relaxed();
        let mut outcomes = Vec::new();
        release_held(&call, &mut outcomes);
        assert!(outcomes.is_empty());
    }
}
