use std::time::Duration;

use deckhand_core::{Point, Rect, WindowInfo, WindowState};
use deckhand_desktop::overlay::{CaptureHideGuard, Overlay};

use crate::{call_scope::CallScope, engine::Engine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyChips {
    Typing,
    Keys(Vec<Vec<String>>),
}

pub fn begin(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    if !frames_window(window) || engine.guard.is_stopped() {
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

pub fn key_chips(engine: &Engine, window: &WindowInfo, chips: KeyChips) {
    let Some(overlay) = &engine.overlay else { return };
    match chips {
        KeyChips::Typing => overlay.typing(window.rect),
        KeyChips::Keys(chords) => overlay.keys(window.rect, chords),
    }
}

pub fn typing(engine: &Engine, window: &WindowInfo) {
    if let Some(overlay) = &engine.overlay {
        overlay.typing(window.rect);
    }
}

pub fn screenshot(engine: &Engine, rect: Rect, window: Option<&WindowInfo>) {
    if window.is_some_and(|window| !window.on_current_desktop) {
        return;
    }
    if let Some(overlay) = &engine.overlay {
        overlay.screenshot(rect);
    }
}

pub fn scan(engine: &Engine, window: &WindowInfo) {
    let showing = engine.config.overlay.show_reads && frames_window(window);
    if let Some(overlay) = engine.overlay.as_ref().filter(|_| showing) {
        overlay.scan(window.rect);
    }
}

pub fn hidden_for_capture(engine: &Engine) -> Option<CaptureHideGuard<'_>> {
    engine.overlay.as_deref().map(Overlay::hidden_for_capture)
}

fn frames_window(window: &WindowInfo) -> bool {
    window.on_current_desktop && window.state != WindowState::Minimized
}

#[cfg(test)]
mod tests {
    use deckhand_core::{WindowRef, WindowState};

    use super::*;
    use crate::test_fixture::window;

    #[test]
    fn only_visible_windows_on_this_desktop_get_a_frame() {
        let shown = window(1, "notepad.exe", "Notes", Rect::new(0, 0, 400, 300));
        assert!(frames_window(&shown));
        let minimized = WindowInfo { state: WindowState::Minimized, ..shown.clone() };
        assert!(!frames_window(&minimized));
        let elsewhere = WindowInfo { on_current_desktop: false, hwnd: WindowRef(2), ..shown };
        assert!(!frames_window(&elsewhere));
    }
}
