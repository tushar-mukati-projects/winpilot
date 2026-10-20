use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
use winpilot_core::{Context, Error, Rect, Result, WindowRef};
use winpilot_win32::{frame_bounds, is_minimized, live_hwnd, rect_from_win32};

use crate::capture_area::CaptureArea;
use crate::frame::{Frame, FrameSource};
use crate::window_print::printed_window;
use crate::window_visibility::shows_on_screen;

pub(crate) fn capture_window(window: WindowRef, screen: impl FnOnce(CaptureArea) -> Result<Frame>) -> Result<Frame> {
    let hwnd = live_hwnd(window)?;
    if is_minimized(hwnd) {
        return Err(Error::invalid("window is minimized"));
    }
    let outer = CaptureArea::new(window_rect(hwnd)?)?;
    let visible = visible_bounds(hwnd, outer)?;
    match printed_window(hwnd, outer.rect()) {
        Some(printed) if !printed.is_all_black() => printed
            .crop(visible.rect())
            .ok_or_else(|| Error::internal("window image does not cover its visible bounds")),
        _ => fall_back_to_screen(window, hwnd, visible, screen),
    }
}

pub(crate) fn window_rect(hwnd: HWND) -> Result<Rect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &raw mut rect) }.context("GetWindowRect")?;
    Ok(rect_from_win32(rect))
}

fn visible_bounds(hwnd: HWND, outer: CaptureArea) -> Result<CaptureArea> {
    let bounds = frame_bounds(hwnd).unwrap_or(outer.rect());
    outer.clip(bounds).ok_or_else(|| Error::invalid("window has no visible area"))
}

fn fall_back_to_screen(
    window: WindowRef,
    hwnd: HWND,
    visible: CaptureArea,
    screen: impl FnOnce(CaptureArea) -> Result<Frame>,
) -> Result<Frame> {
    if !shows_on_screen(hwnd, visible.rect()) {
        let message = format!("{window} could not be captured while covered or hidden; bring it to the front");
        return Err(Error::invalid(message));
    }
    let mut frame = screen(visible)?;
    frame.source = FrameSource::ScreenFallback;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;
    use winpilot_win32::{is_visible, top_level_windows};

    use super::*;

    const HIDDEN_WINDOWS_TO_TRY: usize = 6;

    fn is_hidden_and_sized(hwnd: HWND) -> bool {
        let sized = window_rect(hwnd).is_ok_and(|rect| rect.width >= 16 && rect.height >= 16);
        sized && !is_visible(hwnd) && !is_minimized(hwnd)
    }

    fn hidden_windows() -> Vec<HWND> {
        let windows = top_level_windows().unwrap();
        windows.into_iter().filter(|hwnd| is_hidden_and_sized(*hwnd)).take(HIDDEN_WINDOWS_TO_TRY).collect()
    }

    #[test]
    fn missing_windows_are_not_found() {
        let error = capture_window(WindowRef(0), |_| Err(Error::internal("unused"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        let error = capture_window(WindowRef(0x7FFF_FFF0), |_| Err(Error::internal("unused"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn hidden_windows_never_borrow_screen_pixels_or_return_black() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        for hwnd in hidden_windows() {
            let window = WindowRef(hwnd.0 as i64);
            match capture_window(window, |_| Err(Error::internal("the screen must not be read"))) {
                Ok(frame) => {
                    assert_eq!(frame.source, FrameSource::Window);
                    assert!(!frame.is_all_black());
                }
                Err(error) => assert!(matches!(error.code, ErrorCode::InvalidArgument | ErrorCode::NotFound)),
            }
        }
    }

    #[test]
    fn hidden_windows_get_the_covered_or_hidden_error() {
        let Some(&hwnd) = hidden_windows().first() else {
            return;
        };
        let window = WindowRef(hwnd.0 as i64);
        let visible = CaptureArea::new(Rect::new(0, 0, 16, 16)).unwrap();
        let error = fall_back_to_screen(window, hwnd, visible, |_| Err(Error::internal("unused"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        let expected = format!("{window} could not be captured while covered or hidden; bring it to the front");
        assert_eq!(error.message, expected);
    }
}
