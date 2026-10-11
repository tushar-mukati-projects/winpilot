use std::time::Duration;

use deckhand_core::{Result, TargetSpec, protocol::MouseButton};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate, locate_in_front},
    motion::{glide_duration, move_pointer},
    settle,
    target::prepare,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const SHORTEST_DRAG: Duration = Duration::from_millis(100);

pub fn hover(engine: &Engine, target: &TargetSpec, glide_ms: Option<u64>, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let located = locate_in_front(engine, &prepared, call)?;
    let point = hit_point(engine, &prepared, &located)?;
    overlay::begin(engine, &located.window, call);
    move_pointer(engine, point, glide_ms)?;
    settle(engine, &located.window, call);
    Ok(Done::by("mouse"))
}

pub fn drag(
    engine: &Engine,
    ends: (&TargetSpec, &TargetSpec),
    glide_ms: Option<u64>,
    call: &CallScope,
) -> Result<Done> {
    let (from, to) = ends;
    let start = prepare(engine, from, call)?;
    let end = prepare(engine, to, call)?;
    let grab = locate_in_front(engine, &start, call)?;
    let grab_point = hit_point(engine, &start, &grab)?;
    let release = locate(engine, &end, deckhand_desktop::window(end.window.hwnd)?, call)?;
    let release_point = hit_point(engine, &end, &release)?;
    overlay::begin(engine, &grab.window, call);
    move_pointer(engine, grab_point, glide_ms)?;
    overlay::click(engine, grab_point, 1);
    let travel = glide_duration(engine.config.cursor.glide_ms, glide_ms).max(SHORTEST_DRAG);
    overlay::glide(engine, grab_point, release_point, travel);
    deckhand_input::drag(grab_point, release_point, MouseButton::Left, travel)?;
    overlay::click(engine, release_point, 1);
    settle(engine, &grab.window, call);
    Ok(Done::by("mouse"))
}
