use std::time::Duration;

use deckhand_core::{Result, protocol::MouseButton};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate, locate_in_front},
    motion::{glide_duration, move_pointer},
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const SHORTEST_DRAG: Duration = Duration::from_millis(100);

pub struct Drag<'a> {
    pub from: &'a Prepared,
    pub to: &'a Prepared,
    pub button: MouseButton,
    pub glide_ms: Option<u64>,
}

pub fn move_to(engine: &Engine, prepared: &Prepared, glide_ms: Option<u64>, call: &CallScope) -> Result<Done> {
    let located = locate_in_front(engine, prepared, call)?;
    let point = hit_point(engine, prepared, &located)?;
    overlay::begin(engine, &located.window, call);
    move_pointer(engine, point, glide_ms)?;
    settle(engine, &located.window, call);
    Ok(Done::by("mouse"))
}

pub fn drag(engine: &Engine, drag: &Drag<'_>, call: &CallScope) -> Result<Done> {
    let grab = locate_in_front(engine, drag.from, call)?;
    let grab_point = hit_point(engine, drag.from, &grab)?;
    let release = locate(engine, drag.to, deckhand_desktop::window(drag.to.window.hwnd)?, call)?;
    let release_point = hit_point(engine, drag.to, &release)?;
    overlay::begin(engine, &grab.window, call);
    move_pointer(engine, grab_point, drag.glide_ms)?;
    overlay::click(engine, grab_point, 1);
    let travel = glide_duration(engine.config.cursor.glide_ms, drag.glide_ms).max(SHORTEST_DRAG);
    overlay::glide(engine, grab_point, release_point, travel);
    deckhand_input::drag(grab_point, release_point, drag.button, travel)?;
    overlay::click(engine, release_point, 1);
    settle(engine, &grab.window, call);
    Ok(Done::by("mouse"))
}
