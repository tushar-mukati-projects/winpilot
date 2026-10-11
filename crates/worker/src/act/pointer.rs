use deckhand_core::{Result, TargetSpec, protocol::MouseButton};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate, locate_in_front},
    overlay, settle,
    target::prepare,
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn hover(engine: &Engine, target: &TargetSpec, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let located = locate_in_front(engine, &prepared, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    deckhand_input::move_to(point)?;
    overlay::flash(engine, point);
    settle(engine, &located.window, call);
    Ok(Done::by("mouse"))
}

pub fn drag(engine: &Engine, from: &TargetSpec, to: &TargetSpec, call: &CallScope) -> Result<Done> {
    let start = prepare(engine, from, call)?;
    let end = prepare(engine, to, call)?;
    let grab = locate_in_front(engine, &start, call)?;
    let grab_point = hit_point(engine, &start, &grab)?;
    let release = locate(engine, &end, deckhand_desktop::window(end.window.hwnd)?, call)?;
    let release_point = hit_point(engine, &end, &release)?;
    overlay::show_target(engine, &grab.window, call);
    deckhand_input::drag(grab_point, release_point, MouseButton::Left)?;
    overlay::flash(engine, release_point);
    settle(engine, &grab.window, call);
    Ok(Done::by("mouse"))
}
