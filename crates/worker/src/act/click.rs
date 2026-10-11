use deckhand_core::{Element, Error, Result, protocol::ClickHow};

use super::{
    click_choice::{ClickOptions, ClickPath, choose_path},
    done::Done,
    mode::Mode,
    mouse_click::click_by_mouse,
    pattern_click::click_by_pattern,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const MOUSE: &str = "mouse";

pub fn step(engine: &Engine, prepared: &Prepared, options: ClickOptions, mode: Mode, call: &CallScope) -> Result<Done> {
    let method = match mode {
        Mode::Foreground => click_prepared(engine, prepared, options, call)?,
        Mode::Background => click_in_background(engine, prepared)?,
    };
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
    if let Some(method) = clicked_by_pattern(engine, prepared, element)? {
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

fn click_in_background(engine: &Engine, prepared: &Prepared) -> Result<&'static str> {
    let element = prepared.require_element("a background click")?;
    clicked_by_pattern(engine, prepared, element)?.ok_or_else(|| {
        Error::unsupported(format!(
            "{} has no invoke, toggle, selection, expand/collapse or default-action pattern, so it can't be clicked without the real mouse",
            element.id
        ))
    })
}

fn clicked_by_pattern(engine: &Engine, prepared: &Prepared, element: &Element) -> Result<Option<&'static str>> {
    let method = click_by_pattern(engine, element)?;
    if method.is_some() && !element.rect.is_empty() && prepared.window.on_current_desktop {
        overlay::click(engine, element.rect.center(), 1);
    }
    Ok(method)
}
