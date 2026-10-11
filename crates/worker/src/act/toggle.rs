use deckhand_core::{Element, ElementRef, Error, Result, TargetSpec};

use super::{
    click_choice::ClickOptions,
    done::Done,
    locate::refreshed,
    mouse_click::click_by_mouse,
    settle,
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn step(engine: &Engine, target: &TargetSpec, on: Option<bool>, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let element = prepared.require_element("toggle")?;
    if let Some(wanted) = on {
        require_readable(element.id, element.state.toggled)?;
        if element.state.toggled == Some(wanted) {
            return Ok(Done::noting(format!("already {}", state_word(Some(wanted)))));
        }
    }
    let method = flip(engine, &prepared, element, call)?;
    settle(engine, &prepared.window, call);
    let state = refreshed(engine, element.id)?.state.toggled;
    check_state(element.id, on, state)?;
    Ok(Done::by(method).with_note(Some(format!("now {}", state_word(state)))))
}

pub fn require_readable(id: ElementRef, state: Option<bool>) -> Result<()> {
    match state {
        Some(_) => Ok(()),
        None => Err(Error::unsupported(format!("{id}'s checked state can't be read; use click instead"))),
    }
}

pub fn check_state(id: ElementRef, wanted: Option<bool>, state: Option<bool>) -> Result<()> {
    match wanted {
        Some(wanted) if state != Some(wanted) => Err(Error::unsupported(format!(
            "{id} should be {} but is {} after toggling; it may have a third state or be controlled by the app",
            state_word(Some(wanted)),
            state_word(state)
        ))),
        _ => Ok(()),
    }
}

pub const fn state_word(state: Option<bool>) -> &'static str {
    match state {
        Some(true) => "on",
        Some(false) => "off",
        None => "neither on nor off",
    }
}

fn flip(engine: &Engine, prepared: &Prepared, element: &Element, call: &CallScope) -> Result<&'static str> {
    if element.patterns.toggle {
        let id = element.id;
        return engine.uia.run(move |uia| uia.toggle(id)).map(|_| "toggle");
    }
    click_by_mouse(engine, prepared, ClickOptions::default(), call).map(|_| "mouse")
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

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
