use std::time::Duration;

use deckhand_core::{Result, protocol::MouseButton};

use super::{
    done::Done,
    hit::hit_point,
    locate::{locate_in_front, locate_in_place},
    motion::Motion,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const SHORTEST_DRAG: Duration = Duration::from_millis(100);

pub(crate) struct Drag<'a> {
    pub(crate) from: &'a Prepared,
    pub(crate) to: &'a Prepared,
    pub(crate) button: MouseButton,
    pub(crate) glide_ms: Option<u64>,
}

pub(crate) fn move_to(engine: &Engine, prepared: &Prepared, glide_ms: Option<u64>, call: &CallScope) -> Result<Done> {
    let located = locate_in_front(engine, prepared, call)?;
    Motion::start(engine, call).approach(prepared, &located, glide_ms)?;
    settle(engine, &located.window, call);
    Ok(Done::by("mouse"))
}

pub(crate) fn drag(engine: &Engine, drag: &Drag<'_>, call: &CallScope) -> Result<Done> {
    let grab = locate_in_front(engine, drag.from, call)?;
    let release = locate_in_place(engine, drag.to, call)?;
    let release_point = hit_point(engine, drag.to, &release)?;
    let motion = Motion::start(engine, call);
    let grab_point = motion.approach(drag.from, &grab, drag.glide_ms)?;
    overlay::click(engine, grab_point, 1);
    let travel = motion.glide_duration(drag.glide_ms).max(SHORTEST_DRAG);
    overlay::glide(engine, grab_point, release_point, travel);
    deckhand_input::drag_guarded(grab_point, release_point, drag.button, travel, || motion.check())?;
    overlay::click(engine, release_point, 1);
    settle(engine, &grab.window, call);
    Ok(Done::by("mouse"))
}
