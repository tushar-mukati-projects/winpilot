use deckhand_core::{Error, Result, TargetSpec, protocol::ClickHow};

use super::{
    click_choice::{ClickOptions, ClickPath, choose_path},
    done::Done,
    mouse_click::click_by_mouse,
    pattern_click::click_by_pattern,
    settle,
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const MOUSE: &str = "mouse";

pub fn step(engine: &Engine, target: &TargetSpec, options: ClickOptions, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let method = click_prepared(engine, &prepared, options, call)?;
    settle(engine, &prepared.window, call);
    Ok(Done::by(method))
}

pub fn click_prepared(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<&'static str> {
    let patterns = prepared.tracked_element().map(|element| element.patterns);
    match choose_path(options, prepared.window.foreground, patterns) {
        ClickPath::Mouse => click_by_mouse(engine, prepared, options, call).map(|_| MOUSE),
        ClickPath::Pattern => pattern_or_fallback(engine, prepared, options, call),
    }
}

fn pattern_or_fallback(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<&'static str> {
    let Some(element) = prepared.tracked_element() else {
        return Err(Error::invalid("how=invoke needs an element target, not a point or a window"));
    };
    if let Some(method) = click_by_pattern(engine, element)? {
        if !element.rect.is_empty() {
            overlay::click(engine, element.rect.center(), 1);
        }
        return Ok(method);
    }
    if options.how == ClickHow::Auto {
        return click_by_mouse(engine, prepared, options, call).map(|_| MOUSE);
    }
    Err(Error::unsupported(format!(
        "{} has no invoke, toggle, selection, expand/collapse or default-action pattern; use how=mouse",
        element.id
    )))
}
