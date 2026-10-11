use deckhand_core::Result;

use super::{done::Done, focus::set_focus_verified, foreground::bring_forward, settle, target::Prepared};
use crate::{call_scope::CallScope, engine::Engine};

pub fn step(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Done> {
    bring_forward(&prepared.window)?;
    let Some(element) = prepared.element_or_point_hit() else {
        settle(engine, &prepared.window, call);
        return Ok(Done::by("window"));
    };
    let landed = set_focus_verified(engine, element.id)?;
    settle(engine, &prepared.window, call);
    let note = (!landed).then(|| format!("focus did not land inside {}", element.id));
    Ok(Done::by("focus").with_note(note))
}
