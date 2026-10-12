use deckhand_core::{Element, Error, Result, protocol::ClickHow};

use super::{
    by_pattern::by_pattern,
    click_choice::{ClickOptions, ClickPath, choose_path, pattern_actions},
    done::Done,
    mode::Mode,
    mouse_click::click_by_mouse,
    pattern_click::click_by_pattern,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const MOUSE: &str = "mouse";

pub(crate) fn step(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    match mode {
        Mode::Foreground => click_prepared(engine, prepared, options, call),
        Mode::Background => click_in_background(engine, prepared, call),
    }
}

pub(crate) fn click_prepared(
    engine: &Engine,
    prepared: &Prepared,
    options: ClickOptions,
    call: &CallScope,
) -> Result<Done> {
    let patterns = prepared.tracked_element().map(|element| element.patterns);
    match choose_path(options, prepared.window.foreground, patterns) {
        ClickPath::Mouse => click_and_settle(engine, prepared, options, call),
        ClickPath::Pattern => pattern_or_fallback(engine, prepared, options, call),
    }
}

fn click_and_settle(engine: &Engine, prepared: &Prepared, options: ClickOptions, call: &CallScope) -> Result<Done> {
    click_by_mouse(engine, prepared, options, call)?;
    settle(engine, &prepared.window, call);
    Ok(Done::by(MOUSE))
}

fn pattern_or_fallback(engine: &Engine, prepared: &Prepared, options: ClickOptions, call: &CallScope) -> Result<Done> {
    let Some(element) = prepared.tracked_element() else {
        return Err(Error::invalid("how=invoke needs an element target, not a point or a window"));
    };
    if let Some(done) = clicked_by_pattern(engine, prepared, element, call)? {
        return Ok(done);
    }
    if options.how == ClickHow::Auto {
        return click_and_settle(engine, prepared, options, call);
    }
    Err(Error::unsupported(format!(
        "{} has no invoke, toggle, selection, expand/collapse or default-action pattern; use how=mouse",
        element.id
    )))
}

fn click_in_background(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Done> {
    let element = prepared.require_element("a background click")?;
    clicked_by_pattern(engine, prepared, element, call)?.ok_or_else(|| {
        Error::unsupported(format!(
            "{} has no invoke, toggle, selection, expand/collapse or default-action pattern, so it can't be clicked without the real mouse",
            element.id
        ))
    })
}

fn clicked_by_pattern(
    engine: &Engine,
    prepared: &Prepared,
    element: &Element,
    call: &CallScope,
) -> Result<Option<Done>> {
    if pattern_actions(element.patterns, element.state.expanded).is_empty() {
        return Ok(None);
    }
    let (method, note) = by_pattern(engine, &prepared.window, call, || click_by_pattern(engine, element))?;
    let Some(method) = method else { return Ok(None) };
    if !element.rect.is_empty() && prepared.window.on_current_desktop {
        overlay::click(engine, element.rect.center(), 1);
    }
    Ok(Some(Done::by(method).with_note(note)))
}
