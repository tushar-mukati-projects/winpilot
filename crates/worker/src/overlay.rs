use std::time::Duration;

use deckhand_core::{Point, Rect, WindowInfo, WindowState};
use deckhand_desktop::overlay::{CaptureHideGuard, Overlay};

use crate::{call_scope::CallScope, engine::Engine};

pub fn begin(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    if !window.on_current_desktop {
        return;
    }
    if let Some(overlay) = &engine.overlay {
        overlay.begin(window.rect);
        call.note_overlay_shown();
    }
}

pub fn end(engine: &Engine) {
    if let Some(overlay) = &engine.overlay {
        overlay.end();
    }
}

pub fn click(engine: &Engine, point: Point, count: u8) {
    if let Some(overlay) = &engine.overlay {
        overlay.click(point, count);
    }
}

pub fn glide(engine: &Engine, from: Point, to: Point, duration: Duration) {
    if let Some(overlay) = &engine.overlay {
        overlay.glide(from, to, duration);
    }
}

pub fn focus_field(engine: &Engine, rect: Rect) {
    if let Some(overlay) = engine.overlay.as_ref().filter(|_| !rect.is_empty()) {
        overlay.focus_field(rect);
    }
}

pub fn keystrokes(engine: &Engine, window: &WindowInfo, chords: Vec<Vec<String>>) {
    if let Some(overlay) = &engine.overlay {
        overlay.keystrokes(window.rect, chords);
    }
}

pub fn typing(engine: &Engine, window: &WindowInfo, characters: usize) {
    if let Some(overlay) = &engine.overlay {
        overlay.typing(window.rect, characters);
    }
}

pub fn screenshot(engine: &Engine, rect: Rect) {
    if let Some(overlay) = &engine.overlay {
        overlay.screenshot(rect);
    }
}

pub fn scan(engine: &Engine, window: &WindowInfo) {
    let showing =
        engine.config.overlay.show_reads && window.state != WindowState::Minimized && window.on_current_desktop;
    if let Some(overlay) = engine.overlay.as_ref().filter(|_| showing) {
        overlay.scan(window.rect);
    }
}

pub fn hidden_for_capture(engine: &Engine) -> Option<CaptureHideGuard<'_>> {
    engine.overlay.as_deref().map(Overlay::hidden_for_capture)
}
