use std::time::{Duration, Instant};

use winpilot_capture::{ocr, ocr_magnified};
use winpilot_core::{Rect, Result, WindowInfo, WindowRef, ocr::OcrLine};

use crate::{call_scope::CallScope, engine::Engine, frames, watch::Watch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcrReading {
    pub(crate) window: WindowRef,
    pub(crate) rect: Rect,
    pub(crate) at: Instant,
    pub(crate) lines: Vec<OcrLine>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Freshness {
    CachedIsFine,
    TakenAfter(Instant),
}

pub(crate) fn window_lines(
    engine: &Engine,
    window: &WindowInfo,
    call: Option<&CallScope>,
    freshness: Freshness,
) -> Result<Vec<OcrLine>> {
    if let Some(reading) = call.and_then(CallScope::recalled_ocr)
        && reusable(&reading, window, freshness, || changed_since(engine, window, reading.at))
    {
        return Ok(reading.lines);
    }
    let at = Instant::now();
    let lines = ocr(&frames::window(engine, window)?.frame)?;
    if let Some(call) = call {
        call.remember_ocr(OcrReading { window: window.hwnd, rect: window.rect, at, lines: lines.clone() });
    }
    Ok(lines)
}

pub(crate) fn magnified_window_lines(engine: &Engine, window: &WindowInfo) -> Result<Vec<OcrLine>> {
    ocr_magnified(&frames::window(engine, window)?.frame)
}

fn reusable(
    reading: &OcrReading,
    window: &WindowInfo,
    freshness: Freshness,
    changed_since: impl FnOnce() -> bool,
) -> bool {
    let fresh_enough = match freshness {
        Freshness::CachedIsFine => true,
        Freshness::TakenAfter(after) => reading.at >= after,
    };
    reading.window == window.hwnd && reading.rect == window.rect && fresh_enough && !changed_since()
}

fn changed_since(engine: &Engine, window: &WindowInfo, at: Instant) -> bool {
    Watch::window(window).wait_for_change(engine, at, Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::window;

    fn reading(at: Instant) -> OcrReading {
        OcrReading { window: WindowRef(5), rect: Rect::new(0, 0, 100, 100), at, lines: Vec::new() }
    }

    fn app() -> WindowInfo {
        window(5, "capcut.exe", "CapCut", Rect::new(0, 0, 100, 100))
    }

    #[test]
    fn a_reading_of_the_same_unchanged_window_is_reused() {
        assert!(reusable(&reading(Instant::now()), &app(), Freshness::CachedIsFine, || false));
    }

    #[test]
    fn a_changed_moved_or_other_window_is_read_again() {
        let now = Instant::now();
        assert!(!reusable(&reading(now), &app(), Freshness::CachedIsFine, || true));
        let moved = WindowInfo { rect: Rect::new(10, 0, 100, 100), ..app() };
        assert!(!reusable(&reading(now), &moved, Freshness::CachedIsFine, || false));
        let other = WindowInfo { hwnd: WindowRef(6), ..app() };
        assert!(!reusable(&reading(now), &other, Freshness::CachedIsFine, || false));
    }

    #[test]
    fn a_reading_older_than_asked_for_is_read_again_without_asking_about_changes() {
        let then = Instant::now();
        let later = then + Duration::from_millis(5);
        let never = || -> bool { panic!("asked about changes") };
        assert!(!reusable(&reading(then), &app(), Freshness::TakenAfter(later), never));
        assert!(reusable(&reading(later), &app(), Freshness::TakenAfter(then), || false));
    }
}
