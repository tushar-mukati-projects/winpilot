use deckhand_core::{Result, TargetSpec, protocol::MouseButton, target::PointSpec};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate, locate_in_front},
    mode::Mode,
    motion::move_pointer,
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    pub button: MouseButton,
    pub glide_ms: Option<u64>,
}

pub fn mouse_down(engine: &Engine, target: Option<&Prepared>, press: Press, call: &CallScope) -> Result<Done> {
    let at_cursor;
    let prepared = if let Some(prepared) = target {
        prepared
    } else {
        at_cursor = cursor_target(engine, call)?;
        &at_cursor
    };
    let located = locate_in_front(engine, prepared, call)?;
    let point = hit_point(engine, prepared, &located)?;
    overlay::begin(engine, &located.window, call);
    move_pointer(engine, point, press.glide_ms)?;
    deckhand_input::button_down(point, press.button)?;
    call.note_held(press.button);
    overlay::click(engine, point, 1);
    Ok(Done::by("mouse"))
}

pub fn mouse_up(engine: &Engine, target: Option<&Prepared>, press: Press, call: &CallScope) -> Result<Done> {
    let here = deckhand_input::cursor_position()?;
    let at_cursor;
    let prepared = if let Some(prepared) = target {
        prepared
    } else {
        at_cursor = cursor_target(engine, call)?;
        &at_cursor
    };
    let window = deckhand_desktop::window(prepared.window.hwnd)?;
    let located = locate(engine, prepared, window, call)?;
    let point = hit_point(engine, prepared, &located)?;
    if point != here {
        move_pointer(engine, point, press.glide_ms)?;
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
