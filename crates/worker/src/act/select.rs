use deckhand_core::{ElementRef, Error, ErrorCode, Result};

use super::{
    done::Done,
    locate::refreshed,
    mode::Mode,
    mouse_select,
    select_check::{deselected_outcome, item_outcome, pattern_outcome},
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn option(engine: &Engine, prepared: &Prepared, option: &str, mode: Mode, call: &CallScope) -> Result<Done> {
    let id = prepared.require_element("set option")?.id;
    match (select_by_pattern(engine, prepared, id, option, call), mode) {
        (Err(error), Mode::Foreground) if error.code == ErrorCode::Unsupported => {
            mouse_select::select(engine, prepared, id, option, call)
        }
        (Err(error), Mode::Background) if error.code == ErrorCode::Unsupported => Err(Error::unsupported(format!(
            "{id} can't choose \"{option}\" through UI Automation, so background mode can't set it"
        ))),
        (result, _) => result,
    }
}

pub fn item(engine: &Engine, prepared: &Prepared, selected: bool, call: &CallScope) -> Result<Done> {
    let id = prepared.require_element("set selected")?.id;
    let method = if selected { select_or_invoke(engine, id)? } else { deselect(engine, id)? };
    settle(engine, &prepared.window, call);
    let now = refreshed(engine, id)?;
    let note = if selected { item_outcome(&now)? } else { deselected_outcome(&now)? };
    Ok(Done::by(method).with_note(note))
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

fn select_or_invoke(engine: &Engine, id: ElementRef) -> Result<&'static str> {
    match engine.uia.run(move |uia| uia.select(id)) {
        Ok(()) => Ok("select"),
        Err(error) if error.code == ErrorCode::Unsupported => {
            engine.uia.run(move |uia| uia.invoke(id)).map(|()| "invoke")
        }
        Err(error) => Err(error),
    }
}

fn deselect(engine: &Engine, id: ElementRef) -> Result<&'static str> {
    engine.uia.run(move |uia| uia.deselect(id)).map(|()| "deselect")
}
