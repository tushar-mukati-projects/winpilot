use deckhand_core::{Result, TargetSpec, protocol::MouseButton, target::PointSpec};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate_in_front, locate_in_place},
    mode::Mode,
    motion::Motion,
    target::{Prepared, given_or_else, prepare},
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Press {
    pub(crate) button: MouseButton,
    pub(crate) glide_ms: Option<u64>,
}

pub(crate) fn mouse_down(engine: &Engine, target: Option<&Prepared>, press: Press, call: &CallScope) -> Result<Done> {
    let prepared = given_or_else(target, || cursor_target(engine, call))?;
    let located = locate_in_front(engine, &prepared, call)?;
    let point = Motion::start(engine, call).approach(&prepared, &located, press.glide_ms)?;
    deckhand_input::button_down(point, press.button)?;
    call.note_held(press.button, point);
    overlay::click(engine, point, 1);
    Ok(Done::by("mouse"))
}

pub(crate) fn mouse_up(engine: &Engine, target: Option<&Prepared>, press: Press, call: &CallScope) -> Result<Done> {
    let here = deckhand_input::cursor_position()?;
    let prepared = given_or_else(target, || cursor_target(engine, call))?;
    let located = locate_in_place(engine, &prepared, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    if point != here {
        Motion::start(engine, call).glide_to(point, press.glide_ms)?;
    }
    deckhand_input::button_up(point, press.button)?;
    call.note_released(press.button);
    overlay::click(engine, point, 1);
    Ok(Done::by("mouse"))
}

fn cursor_target(engine: &Engine, call: &CallScope) -> Result<Prepared> {
    let here = deckhand_input::cursor_position()?;
    let spot = TargetSpec::Point(PointSpec { x: here.x, y: here.y, img: None });
    prepare(engine, &spot, Mode::Foreground, call)
}
