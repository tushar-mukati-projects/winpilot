use winpilot_capture::{Frame, FrameSource};
use winpilot_core::{Error, Rect, Result, WindowInfo, WindowRef, WindowState, format::other_desktop};
use winpilot_desktop::overlay::CaptureHideGuard;

use crate::{blackout, cover, engine::Engine, overlay};

const OVERLAY_MAY_SHOW: &str =
    "WinPilot's overlay could not be confirmed hidden in time, so it may appear in this capture";

pub(crate) struct Captured {
    pub(crate) frame: Frame,
    pub(crate) note: Option<&'static str>,
}

impl Captured {
    const fn clean(frame: Frame) -> Self {
        Self { frame, note: None }
    }

    fn map(self, change: impl FnOnce(Frame) -> Result<Frame>) -> Result<Self> {
        Ok(Self { frame: change(self.frame)?, note: self.note })
    }
}

pub(crate) fn region(engine: &Engine, rect: Rect) -> Result<Captured> {
    screened(engine, None, || engine.capturer.screen(rect))
}

pub(crate) fn monitor(engine: &Engine, index: usize) -> Result<Captured> {
    screened(engine, None, || engine.capturer.monitor(index))
}

pub(crate) fn window(engine: &Engine, window: &WindowInfo) -> Result<Captured> {
    capture_window(engine, window, None)
}

pub(crate) fn element(engine: &Engine, window: &WindowInfo, rect: Rect) -> Result<Captured> {
    let area = rect.intersection(window.rect).ok_or_else(outside_window)?;
    capture_window(engine, window, Some(area))
}

fn own_popup_in_front(engine: &Engine, window: &WindowInfo) -> bool {
    winpilot_desktop::foreground_window()
        .is_some_and(|front| front != window.hwnd && !cover::window_covers(engine, window.hwnd, front))
}

fn capture_window(engine: &Engine, window: &WindowInfo, area: Option<Rect>) -> Result<Captured> {
    engine.guard.check_window(window)?;
    if window.state == WindowState::Minimized {
        return Err(Error::invalid(format!(
            "{} is minimized; restore it first (act window action=restore)",
            window.hwnd
        )));
    }
    if !window.on_current_desktop {
        return elsewhere(engine, window, area).map(Captured::clean);
    }
    if window.foreground || own_popup_in_front(engine, window) {
        let rect = area.unwrap_or(window.rect);
        return screened(engine, Some(window.hwnd), || engine.capturer.screen(rect));
    }
    let captured = hidden_from_overlay(engine, || engine.capturer.window(window.hwnd))?;
    let captured = match area {
        Some(area) => captured.map(|frame| frame.crop(area).ok_or_else(outside_window))?,
        None => captured,
    };
    if captured.frame.source == FrameSource::Window {
        return Ok(captured);
    }
    captured.map(|frame| blacked_out(engine, frame, Some(window.hwnd), Vec::new()))
}

fn elsewhere(engine: &Engine, window: &WindowInfo, area: Option<Rect>) -> Result<Frame> {
    let printed = engine.capturer.window(window.hwnd).ok();
    let drawn = printed.filter(|frame| frame.source == FrameSource::Window && !frame.is_uniform());
    let frame = drawn.ok_or_else(|| not_drawn(window))?;
    match area {
        Some(area) => frame.crop(area).ok_or_else(outside_window),
        None => Ok(frame),
    }
}

fn not_drawn(window: &WindowInfo) -> Error {
    let desktop = other_desktop(window).unwrap_or_else(|| "another desktop".to_string());
    Error::unsupported(format!(
        "{} is on {desktop}; Windows doesn't draw it there, so try observe mode=text or move it to this desktop",
        window.hwnd
    ))
}

fn screened(engine: &Engine, above: Option<WindowRef>, capture: impl FnOnce() -> Result<Frame>) -> Result<Captured> {
    let covered_before = denied_rects(engine, above)?;
    let captured = hidden_from_overlay(engine, capture)?;
    captured.map(|frame| blacked_out(engine, frame, above, covered_before))
}

fn hidden_from_overlay(engine: &Engine, capture: impl FnOnce() -> Result<Frame>) -> Result<Captured> {
    let hidden = overlay::hidden_for_capture(engine);
    let frame = capture()?;
    Ok(Captured { frame, note: hidden.as_ref().and_then(unconfirmed_note) })
}

fn unconfirmed_note(hidden: &CaptureHideGuard<'_>) -> Option<&'static str> {
    (hidden.is_hiding() && !hidden.is_confirmed()).then_some(OVERLAY_MAY_SHOW)
}

fn blacked_out(engine: &Engine, mut frame: Frame, above: Option<WindowRef>, mut covered: Vec<Rect>) -> Result<Frame> {
    covered.extend(denied_rects(engine, above)?);
    blackout::fill_black(&mut frame, &covered);
    Ok(frame)
}

fn denied_rects(engine: &Engine, above: Option<WindowRef>) -> Result<Vec<Rect>> {
    let windows = winpilot_desktop::all_visible_windows()?;
    Ok(blackout::covering_rects(&windows, |window| engine.guard.denies_titled(&window.process, &window.title), above))
}

fn outside_window() -> Error {
    Error::invalid("the element is outside its window's visible area")
}
