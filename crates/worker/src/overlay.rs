use std::time::Duration;

use deckhand_core::{Point, Rect, WindowInfo, WindowState};
use deckhand_desktop::overlay::{Activity, CaptureHideGuard, Cue, OutOfSight, Overlay};

pub(crate) use crate::overlay_plan::KeyChips;
use crate::{
    app_label::app_label,
    call_scope::CallScope,
    cover,
    engine::Engine,
    overlay_plan::{Draw, Effect, Staged, View, draws, view},
};

pub(crate) fn begin(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    if engine.guard.is_stopped() {
        return;
    }
    let Some(overlay) = &engine.overlay else { return };
    let view = view_at(engine, call, window, window.rect.center());
    call.stage(Staged { window: window.clone(), view });
    if show_stage(overlay, window, view) {
        call.note_overlay_shown();
    }
}

pub(crate) fn end(engine: &Engine) {
    if let Some(overlay) = &engine.overlay {
        overlay.end();
    }
}

pub(crate) fn click(engine: &Engine, call: &CallScope, point: Point, count: u8) {
    show(engine, call, Effect::Click(point, count));
}

pub(crate) fn glide(engine: &Engine, call: &CallScope, from: Point, to: Point, duration: Duration) {
    show(engine, call, Effect::Glide(from, to, duration));
}

pub(crate) fn drag(engine: &Engine, call: &CallScope, from: Point, to: Point, duration: Duration) {
    show(engine, call, Effect::Drag(from, to, duration));
}

pub(crate) fn scroll(engine: &Engine, call: &CallScope, point: Option<Point>) {
    show(engine, call, Effect::Scroll(point));
}

pub(crate) fn focus_field(engine: &Engine, call: &CallScope, rect: Rect) {
    if !rect.is_empty() {
        show(engine, call, Effect::Focus(rect));
    }
}

pub(crate) fn key_chips(engine: &Engine, call: &CallScope, window: &WindowInfo, chips: KeyChips) {
    show(engine, call, Effect::Chips(window.rect, chips));
}

pub(crate) fn typing(engine: &Engine, call: &CallScope, window: &WindowInfo) {
    key_chips(engine, call, window, KeyChips::Typing);
}

pub(crate) fn screenshot(engine: &Engine, rect: Rect, window: Option<&WindowInfo>) {
    if window.is_some_and(|window| !in_sight(engine, window, rect.center())) {
        return;
    }
    if let Some(overlay) = &engine.overlay {
        overlay.screenshot(rect);
    }
}

pub(crate) fn scan(engine: &Engine, window: &WindowInfo) {
    if !engine.config.overlay.show_reads {
        return;
    }
    let Some(overlay) = &engine.overlay else { return };
    if in_sight(engine, window, window.rect.center()) {
        overlay.scan(window.rect);
    }
}

pub(crate) fn hidden_for_capture(engine: &Engine) -> Option<CaptureHideGuard<'_>> {
    engine.overlay.as_deref().map(Overlay::hidden_for_capture)
}

fn show(engine: &Engine, call: &CallScope, effect: Effect) {
    let Some(overlay) = &engine.overlay else { return };
    let spot = effect.spot();
    match restaged(engine, call, overlay, spot) {
        Some(staged) => draw_all(overlay, Some(&staged.window), draws(staged.view, effect)),
        None if !call.in_background() => draw_all(overlay, None, draws(View::Driven, effect)),
        None => {}
    }
}

fn restaged(engine: &Engine, call: &CallScope, overlay: &Overlay, spot: Option<Point>) -> Option<Staged> {
    let staged = call.staged()?;
    let Some(spot) = spot.filter(|_| staged.view != View::Driven) else { return Some(staged) };
    let now = view_at(engine, call, &staged.window, spot);
    if now == staged.view {
        return Some(staged);
    }
    let moved = Staged { view: now, ..staged };
    if show_stage(overlay, &moved.window, now) {
        call.note_overlay_shown();
    }
    call.stage(moved.clone());
    Some(moved)
}

fn show_stage(overlay: &Overlay, window: &WindowInfo, view: View) -> bool {
    match view {
        View::Driven if !frames_window(window) => return false,
        View::Driven => overlay.begin(window.rect),
        View::Watched => overlay.begin_in_background(window.rect),
        View::Aside(reason) => overlay.background_activity(activity(window, reason, None)),
    }
    true
}

fn draw_all(overlay: &Overlay, window: Option<&WindowInfo>, planned: Vec<Draw>) {
    for draw in planned {
        match draw {
            Draw::Click(point, count) => overlay.click(point, count),
            Draw::Ripple(point, count) => overlay.ripple(point, count),
            Draw::Glide(from, to, duration) => overlay.glide(from, to, duration),
            Draw::Focus(rect) => overlay.focus_field(rect),
            Draw::Chips(rect, KeyChips::Typing) => overlay.typing(rect),
            Draw::Chips(rect, KeyChips::Keys(chords)) => overlay.keys(rect, chords),
            Draw::Pill(reason, cue) => {
                if let Some(window) = window {
                    overlay.background_activity(activity(window, reason, Some(cue)));
                }
            }
        }
    }
}

fn view_at(engine: &Engine, call: &CallScope, window: &WindowInfo, spot: Point) -> View {
    view(call.in_background(), window, || cover::uncovered_at(engine, window, spot))
}

fn activity(window: &WindowInfo, out_of_sight: OutOfSight, cue: Option<Cue>) -> Activity {
    Activity { app: app_label(window), out_of_sight, cue }
}

fn in_sight(engine: &Engine, window: &WindowInfo, spot: Point) -> bool {
    frames_window(window) && cover::uncovered_at(engine, window, spot)
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

    #[test]
    fn the_pill_names_the_app_where_it_is_and_what_was_done() {
        let capcut = window(1, "CapCut.exe", "CapCut", Rect::new(0, 0, 400, 300));
        let shown = activity(&capcut, OutOfSight::Behind, Some(Cue::Click(1)));
        assert_eq!(
            shown,
            Activity { app: "CapCut".to_string(), out_of_sight: OutOfSight::Behind, cue: Some(Cue::Click(1)) }
        );
    }
}
