use deckhand_core::{ElementRef, Error, Result};

use super::{
    focus::{Focused, focus_is_inside},
    foreground::require_foreground,
};
use crate::{engine::Engine, window_classes::is_popup};

pub fn ensure_keys_land(engine: &Engine, focused: &Focused) -> Result<()> {
    if !is_popup(&focused.window) {
        require_foreground(focused.window.hwnd)?;
    }
    let Some(element) = &focused.element else { return Ok(()) };
    if focus_is_inside(engine, element.id)? {
        return Ok(());
    }
    Err(focus_missed(element.id))
}

fn focus_missed(id: ElementRef) -> Error {
    Error::unsupported(format!(
        "keyboard focus did not move into {id}, so no keys were sent; click it first or use set"
    ))
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    #[test]
    fn a_missed_focus_explains_that_nothing_was_typed() {
        let error = focus_missed(ElementRef(12));
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.message.starts_with("keyboard focus did not move into e12, so no keys were sent"));
    }
}
