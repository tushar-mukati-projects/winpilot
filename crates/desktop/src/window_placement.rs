use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Context, Error, Rect, Result, WindowRef};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
};

use crate::{
    frame::FrameInsets,
    timeouts::{later, poll_until},
    window_geometry::{frame_bounds, outer_rect, visible_rect},
    window_identity::live_hwnd,
    window_restore::{ensure_responsive, restore_to_normal},
};

const PLACEMENT_TOLERANCE: i64 = 2;
const PLACEMENT_PASSES: usize = 2;
const PLACEMENT_TIMEOUT: Duration = Duration::from_secs(1);
const PLACEMENT_POLL: Duration = Duration::from_millis(10);
const SETTLE_TIME: Duration = Duration::from_millis(40);

pub fn move_window(hwnd: WindowRef, rect: Rect) -> Result<()> {
    if rect.is_empty() {
        return Err(Error::invalid("a window needs a positive width and height"));
    }
    let handle = live_hwnd(hwnd)?;
    ensure_responsive(handle, hwnd)?;
    restore_to_normal(handle, hwnd)?;
    for _ in 0..PLACEMENT_PASSES {
        request_placement(handle, hwnd, rect)?;
        if wait_for_placement(handle, rect) {
            return Ok(());
        }
    }
    Err(placement_error(handle, hwnd, rect))
}

fn request_placement(hwnd: HWND, window: WindowRef, visible_target: Rect) -> Result<()> {
    let placement = outer_placement(hwnd, visible_target);
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    unsafe { SetWindowPos(hwnd, None, placement.x, placement.y, placement.width, placement.height, flags) }
        .context(&format!("moving {window}"))
}

fn outer_placement(hwnd: HWND, visible_target: Rect) -> Rect {
    let insets = match (outer_rect(hwnd), frame_bounds(hwnd)) {
        (Some(outer), Some(visible)) => FrameInsets::between(outer, visible),
        _ => FrameInsets::default(),
    };
    insets.outer_for(visible_target)
}

fn wait_for_placement(hwnd: HWND, target: Rect) -> bool {
    let deadline = later(Instant::now(), PLACEMENT_TIMEOUT);
    let placed = || visible_rect(hwnd).is_some_and(|actual| within_tolerance(actual, target));
    poll_until(deadline, PLACEMENT_POLL, &placed) && stays_placed(placed)
}

fn stays_placed(placed: impl Fn() -> bool) -> bool {
    thread::sleep(SETTLE_TIME);
    placed()
}

fn within_tolerance(actual: Rect, target: Rect) -> bool {
    [(actual.x, target.x), (actual.y, target.y), (actual.width, target.width), (actual.height, target.height)]
        .into_iter()
        .all(|(seen, wanted)| (i64::from(seen) - i64::from(wanted)).abs() <= PLACEMENT_TOLERANCE)
}

fn placement_error(hwnd: HWND, window: WindowRef, target: Rect) -> Error {
    if let Err(hung) = ensure_responsive(hwnd, window) {
        return hung;
    }
    let actual = visible_rect(hwnd).map_or_else(|| "an unknown place".to_string(), describe);
    Error::unsupported(format!(
        "{window} stayed at {actual} instead of {}; the app may limit its size or position",
        describe(target)
    ))
}

fn describe(rect: Rect) -> String {
    format!("{}x{} at ({}, {})", rect.width, rect.height, rect.x, rect.y)
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;
    use crate::window_list::foreground_window;

    #[test]
    fn rects_within_two_pixels_count_as_placed() {
        let target = Rect::new(100, 100, 800, 600);
        assert!(within_tolerance(target, target));
        assert!(within_tolerance(Rect::new(102, 98, 802, 598), target));
        assert!(!within_tolerance(Rect::new(103, 100, 800, 600), target));
        assert!(!within_tolerance(Rect::new(100, 100, 800, 603), target));
    }

    #[test]
    fn tolerance_check_survives_extreme_values() {
        assert!(!within_tolerance(Rect::new(i32::MIN, 0, 1, 1), Rect::new(i32::MAX, 0, 1, 1)));
    }

    #[test]
    fn placement_error_describes_both_rects() {
        let error = placement_error(HWND::default(), WindowRef(7), Rect::new(0, 0, 640, 480));
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.message.starts_with("w7 stayed at an unknown place instead of 640x480 at (0, 0)"));
    }

    #[test]
    fn missing_or_empty_targets_are_rejected() {
        assert_eq!(move_window(WindowRef(0), Rect::new(0, 0, 10, 10)).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(move_window(WindowRef(0), Rect::new(0, 0, 0, 10)).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    #[ignore = "moves a real window"]
    fn move_foreground_window_keeps_size() {
        let foreground = foreground_window().unwrap();
        let info = crate::window_list::window(foreground).unwrap();
        move_window(foreground, info.rect).unwrap();
    }
}
