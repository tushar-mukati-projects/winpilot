use deckhand_core::{Result, TargetSpec};

use super::{done::Done, focus::set_focus_verified, settle, target::prepare, window::bring_forward};
use crate::{call_scope::CallScope, engine::Engine};

pub fn step(engine: &Engine, target: &TargetSpec, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    bring_forward(&prepared.window)?;
    let Some(element) = prepared.element() else {
        settle(engine, &prepared.window, call);
        return Ok(Done::by("window"));
    };
    let landed = set_focus_verified(engine, element.id)?;
    settle(engine, &prepared.window, call);
    let note = (!landed).then(|| format!("focus did not land inside {}", element.id));
    Ok(Done::by("focus").with_note(note))
}
