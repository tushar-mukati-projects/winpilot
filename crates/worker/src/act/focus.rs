use deckhand_core::{Element, ElementRef, Error, ErrorCode, Result, WindowInfo};

use super::{
    click_choice::ClickOptions,
    foreground::bring_forward,
    landing::ensure_keys_land,
    mouse_click::click_by_mouse,
    target::{Prepared, foreground_app},
};
use crate::{call_scope::CallScope, engine::Engine, wait::is_gone};

#[derive(Debug, Clone)]
pub struct Focused {
    pub window: WindowInfo,
    pub element: Option<Element>,
    pub clicked: bool,
}

pub fn keyboard_target(engine: &Engine, target: Option<&Prepared>, call: &CallScope) -> Result<Focused> {
    let focused = match target {
        None => Focused { window: foreground_app(engine, call)?, element: None, clicked: false },
        Some(prepared) => focus_prepared(engine, prepared, call)?,
    };
    ensure_keys_land(engine, &focused)?;
    Ok(focused)
}

pub fn focus_prepared(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Focused> {
    let window = bring_forward(&prepared.window)?;
    let Some(element) = prepared.element().cloned() else {
        return Ok(Focused { window, element: None, clicked: false });
    };
    if set_focus_verified(engine, element.id)? {
        return Ok(Focused { window, element: Some(element), clicked: false });
    }
    click_by_mouse(engine, prepared, ClickOptions::default(), call)?;
    Ok(Focused { window, element: Some(element), clicked: true })
}

pub fn set_focus_verified(engine: &Engine, id: ElementRef) -> Result<bool> {
    match engine.uia.run(move |uia| uia.set_focus(id)) {
        Err(error) if must_surface(&error) => Err(error),
        _ => focus_is_inside(engine, id),
    }
}

pub fn focus_is_inside(engine: &Engine, id: ElementRef) -> Result<bool> {
    engine.uia.run(move |uia| {
        let Some(focused) = uia.focused()? else { return Ok(false) };
        match uia.contains(id, focused.id) {
            Err(error) if must_surface(&error) => Err(error),
            result => Ok(result.unwrap_or(false)),
        }
    })
}

fn must_surface(error: &Error) -> bool {
    is_gone(error) || error.code == ErrorCode::Ambiguous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gone_and_ambiguous_refs_are_never_swallowed() {
        assert!(must_surface(&Error::new(ErrorCode::Ambiguous, "two matches")));
        assert!(must_surface(&Error::new(ErrorCode::StaleRef, "gone")));
        assert!(!must_surface(&Error::unsupported("no focus")));
    }
}
