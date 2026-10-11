use deckhand_core::{ElementRef, ErrorCode, Result, TargetSpec};

use super::{
    done::Done,
    locate::refreshed,
    mouse_select,
    select_check::{item_outcome, pattern_outcome},
    settle,
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn step(engine: &Engine, target: &TargetSpec, option: Option<&str>, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let id = prepared.require_element("select")?.id;
    match option {
        Some(option) => select_option(engine, &prepared, id, option, call),
        None => select_item(engine, &prepared, id, call),
    }
}

pub fn select_by_pattern(
    engine: &Engine,
    prepared: &Prepared,
    id: ElementRef,
    option: &str,
    call: &CallScope,
) -> Result<Done> {
    let wanted = option.to_string();
    engine.uia.run(move |uia| uia.select_option(id, &wanted))?;
    settle(engine, &prepared.window, call);
    let note = pattern_outcome(&refreshed(engine, id)?, option)?;
    Ok(Done::by("select").with_note(note))
}

fn select_option(engine: &Engine, prepared: &Prepared, id: ElementRef, option: &str, call: &CallScope) -> Result<Done> {
    match select_by_pattern(engine, prepared, id, option, call) {
        Err(error) if error.code == ErrorCode::Unsupported => mouse_select::select(engine, prepared, id, option, call),
        result => result,
    }
}

fn select_item(engine: &Engine, prepared: &Prepared, id: ElementRef, call: &CallScope) -> Result<Done> {
    let method = select_or_invoke(engine, id)?;
    settle(engine, &prepared.window, call);
    let note = item_outcome(&refreshed(engine, id)?)?;
    Ok(Done::by(method).with_note(note))
}

fn select_or_invoke(engine: &Engine, id: ElementRef) -> Result<&'static str> {
    match engine.uia.run(move |uia| uia.select(id)) {
        Ok(()) => Ok("select"),
        Err(error) if error.code == ErrorCode::Unsupported => {
            engine.uia.run(move |uia| uia.invoke(id)).map(|()| "invoke")
        }
        Err(error) => Err(error),
    }
}
