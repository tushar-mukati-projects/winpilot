use winpilot_capture::{Frame, FrameSource};
use winpilot_core::{Error, ErrorCode, Point, Rect, Result, WindowInfo, WindowRef, WindowState, format::other_desktop};
use winpilot_desktop::{VisibleWindow, overlay::CaptureHideGuard};
use winpilot_win32::{class_name, hwnd_of, owning_pid};

use crate::{blackout, cover, engine::Engine, overlay, wait::unless_gone};

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

pub(crate) fn window_with_popups(engine: &Engine, window: &WindowInfo) -> Result<Captured> {
    let windows = winpilot_desktop::all_visible_windows()?;
    let popups = cover::popups(engine, window, &windows);
    if popups.is_empty() {
        return capture_window(engine, window, None);
    }
    engine.guard.check_window(window)?;
    let area = popups.iter().fold(window.rect, |area, popup| area.union(popup.rect));
    if window.foreground || own_popup_in_front(engine, window) {
        let shown: Vec<Rect> = popups.iter().map(|popup| popup.rect).chain([window.rect]).collect();
        return screened(engine, Some(window.hwnd), || engine.capturer.screen(area))?.map(|frame| only(&frame, &shown));
    }
    printed_with_popups(engine, window.hwnd, &popups, area)
}

pub(crate) fn element(engine: &Engine, window: &WindowInfo, rect: Rect) -> Result<Captured> {
    let area = rect.intersection(window.rect).ok_or_else(outside_window)?;
    capture_window(engine, window, Some(area))
}

fn own_popup_in_front(engine: &Engine, window: &WindowInfo) -> bool {
    winpilot_desktop::foreground_window().is_some_and(|front| {
        let handle = hwnd_of(front);
        cover::is_popup_of(engine, window, front, owning_pid(handle), &class_name(handle))
    })
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
    let captured = printed(engine, window.hwnd)?;
    match area {
        Some(area) => captured.map(|frame| frame.crop(area).ok_or_else(outside_window)),
        None => Ok(captured),
    }
}

fn printed(engine: &Engine, hwnd: WindowRef) -> Result<Captured> {
    let captured = hidden_from_overlay(engine, || engine.capturer.window(hwnd))?;
    if captured.frame.source == FrameSource::Window {
        return Ok(captured);
    }
    captured.map(|frame| blacked_out(engine, frame, Some(hwnd), Vec::new()))
}

fn printed_with_popups(engine: &Engine, hwnd: WindowRef, popups: &[&VisibleWindow], area: Rect) -> Result<Captured> {
    let target = printed(engine, hwnd)?;
    let mut frame = black(area, target.frame.source);
    frame.paste(&target.frame)?;
    let allowed = popups.iter().rev().filter(|popup| !engine.guard.denies_titled(&popup.process, &popup.title));
    for popup in allowed {
        if let Some(drawn) = unless_hidden(printed(engine, popup.hwnd))? {
            frame.paste(&drawn.frame)?;
        }
    }
    Ok(Captured { frame, note: target.note })
}

fn unless_hidden(drawn: Result<Captured>) -> Result<Option<Captured>> {
    match unless_gone(drawn) {
        Err(error) if error.code == ErrorCode::InvalidArgument => Ok(None),
        other => other,
    }
}

fn only(frame: &Frame, shown: &[Rect]) -> Result<Frame> {
    let mut kept = black(frame.rect(), frame.source);
    for piece in shown.iter().filter_map(|rect| frame.crop(*rect)) {
        kept.paste(&piece)?;
    }
    Ok(kept)
}

fn black(area: Rect, source: FrameSource) -> Frame {
    Frame { source, ..Frame::black(Point::new(area.x, area.y), area.width as u32, area.height as u32) }
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

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn a_screen_grab_keeps_only_the_window_and_its_popups() {
        let grab = Frame { bgra: WHITE.repeat(6 * 4), ..black(Rect::new(-2, 10, 6, 4), FrameSource::Screen) };
        let kept = only(&grab, &[Rect::new(-2, 10, 3, 2), Rect::new(2, 12, 5, 5)]).unwrap();
        let rows: Vec<Vec<bool>> = kept
            .bgra
            .as_chunks::<4>()
            .0
            .chunks(kept.width as usize)
            .map(|row| row.iter().map(|pixel| *pixel == WHITE).collect())
            .collect();
        let (top, bottom) = ([true, true, true, false, false, false], [false, false, false, false, true, true]);
        assert_eq!(rows, [top, top, bottom, bottom]);
        assert_eq!((kept.rect(), kept.source), (grab.rect(), FrameSource::Screen));
    }

    #[test]
    fn a_popup_that_closed_or_is_covered_is_skipped_but_other_failures_are_errors() {
        assert!(unless_hidden(printed(crate::test_fixture::engine(), WindowRef(0))).unwrap().is_none());
        assert!(unless_hidden(Err(Error::invalid("covered"))).unwrap().is_none());
        assert_eq!(
            unless_hidden(Err(Error::internal("broken"))).err().map(|error| error.code),
            Some(ErrorCode::Internal)
        );
        let drawn = Captured::clean(black(Rect::new(0, 0, 2, 2), FrameSource::Window));
        assert!(unless_hidden(Ok(drawn)).unwrap().is_some());
    }

    #[test]
    #[ignore = "captures a user window"]
    fn a_shown_popup_that_fails_to_print_is_an_error() {
        let windows = winpilot_win32::top_level_windows().unwrap();
        let minimized = windows
            .into_iter()
            .find(|hwnd| winpilot_win32::is_visible(*hwnd) && winpilot_win32::is_minimized(*hwnd))
            .expect("no minimized window to capture");
        let drawn = printed(crate::test_fixture::engine(), winpilot_win32::ref_of(minimized));
        assert_eq!(unless_hidden(drawn).err().map(|error| error.code), Some(ErrorCode::InvalidArgument));
    }
}
