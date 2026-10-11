use deckhand_capture::{Frame, FrameSource};
use deckhand_core::{Error, Rect, Result, WindowInfo, WindowRef, WindowState, format::other_desktop};

use crate::{blackout, engine::Engine, overlay, uniform_frame::is_uniform};

pub fn region(engine: &Engine, rect: Rect) -> Result<Frame> {
    screened(engine, None, || engine.capturer.screen(rect))
}

pub fn monitor(engine: &Engine, index: usize) -> Result<Frame> {
    screened(engine, None, || engine.capturer.monitor(index))
}

pub fn window(engine: &Engine, window: &WindowInfo) -> Result<Frame> {
    capture_window(engine, window, None)
}

pub fn element(engine: &Engine, window: &WindowInfo, rect: Rect) -> Result<Frame> {
    let area = rect.intersection(window.rect).ok_or_else(outside_window)?;
    capture_window(engine, window, Some(area))
}

fn capture_window(engine: &Engine, window: &WindowInfo, area: Option<Rect>) -> Result<Frame> {
    engine.guard.check_window(window)?;
    if window.state == WindowState::Minimized {
        return Err(Error::invalid(format!(
            "{} is minimized; restore it first (act window action=restore)",
            window.hwnd
        )));
    }
    if !window.on_current_desktop {
        return elsewhere(engine, window, area);
    }
    if window.foreground {
        let rect = area.unwrap_or(window.rect);
        return screened(engine, Some(window.hwnd), || engine.capturer.screen(rect));
    }
    let frame = {
        let _hidden = overlay::hidden_for_capture(engine);
        engine.capturer.window(window.hwnd)?
    };
    let frame = match area {
        Some(area) => frame.crop(area).ok_or_else(outside_window)?,
        None => frame,
    };
    if frame.source == FrameSource::Window {
        return Ok(frame);
    }
    blacked_out(engine, frame, Some(window.hwnd), Vec::new())
}

fn elsewhere(engine: &Engine, window: &WindowInfo, area: Option<Rect>) -> Result<Frame> {
    let printed = engine.capturer.window(window.hwnd).ok();
    let drawn = printed.filter(|frame| frame.source == FrameSource::Window && !is_uniform(&frame.bgra));
    let frame = drawn.ok_or_else(|| not_drawn(window))?;
    match area {
        Some(area) => frame.crop(area).ok_or_else(outside_window),
        None => Ok(frame),
    }
}

pub fn not_drawn(window: &WindowInfo) -> Error {
    let desktop = other_desktop(window).unwrap_or_else(|| "another desktop".to_string());
    Error::unsupported(format!(
        "{} is on {desktop}; Windows doesn't draw it there, so try observe mode=text or move it to this desktop",
        window.hwnd
    ))
}

fn screened(engine: &Engine, above: Option<WindowRef>, capture: impl FnOnce() -> Result<Frame>) -> Result<Frame> {
    let covered_before = denied_rects(engine, above)?;
    let frame = {
        let _hidden = overlay::hidden_for_capture(engine);
        capture()?
    };
    blacked_out(engine, frame, above, covered_before)
}

fn blacked_out(engine: &Engine, mut frame: Frame, above: Option<WindowRef>, mut covered: Vec<Rect>) -> Result<Frame> {
    covered.extend(denied_rects(engine, above)?);
    blackout::fill_black(&mut frame, &covered);
    Ok(frame)
}

fn denied_rects(engine: &Engine, above: Option<WindowRef>) -> Result<Vec<Rect>> {
    let windows = deckhand_desktop::all_visible_windows()?;
    Ok(blackout::covering_rects(&windows, |window| engine.guard.denies_titled(&window.process, &window.title), above))
}

fn outside_window() -> Error {
    Error::invalid("the element is outside its window's visible area")
}
