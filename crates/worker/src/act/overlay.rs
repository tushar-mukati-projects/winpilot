use deckhand_core::{Point, WindowInfo};

use crate::{call_scope::CallScope, engine::Engine};

pub fn show_target(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    if let Some(overlay) = &engine.overlay {
        overlay.show_target(window.rect);
        call.note_overlay_shown();
    }
}

pub fn flash(engine: &Engine, point: Point) {
    if let Some(overlay) = &engine.overlay {
        overlay.flash_point(point);
    }
}

pub fn hide(engine: &Engine) {
    if let Some(overlay) = &engine.overlay {
        overlay.hide();
    }
}
