use deckhand_core::Result;

use super::{
    click_choice::ClickOptions,
    hit::hit_point,
    locate::{Located, locate, locate_in_front},
    overlay,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn click_by_mouse(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Located> {
    let located = locate_in_front(engine, prepared, call)?;
    click_located(engine, prepared, located, options)
}

pub fn click_in_place(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Located> {
    let window = deckhand_desktop::window(prepared.window.hwnd)?;
    let located = locate(engine, prepared, window, call)?;
    click_located(engine, prepared, located, options)
}

fn click_located(engine: &Engine, prepared: &Prepared, located: Located, options: ClickOptions) -> Result<Located> {
    let point = hit_point(engine, prepared, &located)?;
    deckhand_input::click(point, options.button, options.count, options.modifiers)?;
    overlay::flash(engine, point);
    Ok(Located { point, ..located })
}
