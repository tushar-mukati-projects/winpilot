use deckhand_core::Result;

use super::{
    click_choice::ClickOptions,
    locate::{Located, locate_in_front, locate_in_place},
    motion::Motion,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

pub(crate) fn click_by_mouse(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Located> {
    let located = locate_in_front(engine, prepared, call)?;
    click_located(engine, prepared, located, options, call)
}

pub(crate) fn click_in_place(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Located> {
    let located = locate_in_place(engine, prepared, call)?;
    click_located(engine, prepared, located, options, call)
}

fn click_located(
    engine: &Engine,
    prepared: &Prepared,
    located: Located,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Located> {
    let point = Motion::start(engine, call).approach(prepared, &located, options.glide_ms)?;
    deckhand_input::click(point, options.button, options.count, options.modifiers)?;
    overlay::click(engine, call, point, options.count);
    Ok(Located { point, ..located })
}
