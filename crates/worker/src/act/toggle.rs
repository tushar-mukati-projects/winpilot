use winpilot_core::{Element, ElementRef, Error, Result};

use super::{
    by_pattern::by_pattern, click_choice::ClickOptions, done::Done, locate::refreshed, mode::Mode,
    mouse_click::click_by_mouse, settle, target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine};

pub(crate) fn step(engine: &Engine, prepared: &Prepared, wanted: bool, mode: Mode, call: &CallScope) -> Result<Done> {
    let element = prepared.require_element("set checked")?;
    require_readable(element.id, element.state.toggled)?;
    if element.state.toggled == Some(wanted) {
        return Ok(Done::noting(format!("already {}", state_word(Some(wanted)))));
    }
    let (method, handed_back) = flip(engine, prepared, element, mode, call)?;
    let state = refreshed(engine, element.id)?.state.toggled;
    check_state(element.id, Some(wanted), state)?;
    Ok(Done::by(method).with_note(Some(format!("now {}", state_word(state)))).with_note(handed_back))
}

fn require_readable(id: ElementRef, state: Option<bool>) -> Result<()> {
    match state {
        Some(_) => Ok(()),
        None => Err(Error::unsupported(format!("{id}'s checked state can't be read; use click instead"))),
    }
}

fn check_state(id: ElementRef, wanted: Option<bool>, state: Option<bool>) -> Result<()> {
    match wanted {
        Some(wanted) if state != Some(wanted) => Err(Error::unsupported(format!(
            "{id} should be {} but is {} after toggling; it may have a third state or be controlled by the app",
            state_word(Some(wanted)),
            state_word(state)
        ))),
        _ => Ok(()),
    }
}

pub(crate) const fn state_word(state: Option<bool>) -> &'static str {
    match state {
        Some(true) => "on",
        Some(false) => "off",
        None => "neither on nor off",
    }
}

fn flip(
    engine: &Engine,
    prepared: &Prepared,
    element: &Element,
    mode: Mode,
    call: &CallScope,
) -> Result<(&'static str, Option<String>)> {
    if element.patterns.toggle {
        let id = element.id;
        let (_, note) = by_pattern(engine, &prepared.window, call, || engine.uia.run(move |uia| uia.toggle(id)))?;
        return Ok(("toggle", note));
    }
    if mode == Mode::Background {
        return Err(Error::unsupported(format!(
            "{} has no toggle pattern, so background mode can't check or uncheck it",
            element.id
        )));
    }
    click_by_mouse(engine, prepared, ClickOptions::default(), call)?;
    settle(engine, &prepared.window, call);
    Ok(("mouse", None))
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    #[test]
    fn reaching_the_wanted_state_passes() {
        assert!(check_state(ElementRef(5), Some(true), Some(true)).is_ok());
        assert!(check_state(ElementRef(5), Some(false), Some(false)).is_ok());
    }

    #[test]
    fn flipping_without_a_wish_always_passes() {
        assert!(check_state(ElementRef(5), None, None).is_ok());
        assert!(check_state(ElementRef(5), None, Some(true)).is_ok());
    }

    #[test]
    fn missing_the_wanted_state_explains_itself() {
        let error = check_state(ElementRef(5), Some(true), None).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(
            error.message.starts_with("e5 should be on but is neither on nor off after toggling"),
            "{}",
            error.message
        );
        assert!(check_state(ElementRef(5), Some(false), Some(true)).is_err());
    }

    #[test]
    fn an_unreadable_state_cannot_be_set_to_a_wish() {
        let error = require_readable(ElementRef(12), None).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(error.message, "e12's checked state can't be read; use click instead");
        assert!(require_readable(ElementRef(12), Some(false)).is_ok());
    }

    #[test]
    fn state_words() {
        assert_eq!(state_word(Some(true)), "on");
        assert_eq!(state_word(Some(false)), "off");
        assert_eq!(state_word(None), "neither on nor off");
    }
}
