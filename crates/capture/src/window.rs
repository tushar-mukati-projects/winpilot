use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindow};
use winpilot_core::{Context, Error, ErrorCode, Rect, Result, WindowRef};

use crate::capture_area::CaptureArea;
use crate::convert::{hwnd_from_ref, rect_from_win32};
use crate::frame::{Frame, FrameSource};
use crate::window_print::printed_window;
use crate::window_visibility::shows_on_screen;

pub(crate) fn capture_window(window: WindowRef, screen: impl FnOnce(CaptureArea) -> Result<Frame>) -> Result<Frame> {
    let hwnd = live_window(window)?;
    if unsafe { IsIconic(hwnd) }.as_bool() {
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

fn live_window(window: WindowRef) -> Result<HWND> {
    let hwnd = hwnd_from_ref(window);
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(Error::new(ErrorCode::NotFound, format!("window {window} does not exist")));
    }
    Ok(hwnd)
}

fn visible_bounds(hwnd: HWND, outer: CaptureArea) -> Result<CaptureArea> {
    let mut rect = RECT::default();
    let size = size_of::<RECT>() as u32;
    let queried = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut rect).cast(), size) };
    let bounds = if queried.is_ok() { rect_from_win32(rect) } else { outer.rect() };
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
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
    use windows_core::BOOL;

    use super::*;

    unsafe extern "system" fn collect_hidden(hwnd: HWND, found: LPARAM) -> BOOL {
        let found = unsafe { &mut *(found.0 as *mut Vec<HWND>) };
        let sized = window_rect(hwnd).is_ok_and(|rect| rect.width >= 16 && rect.height >= 16);
        if sized && !unsafe { IsWindowVisible(hwnd) }.as_bool() && !unsafe { IsIconic(hwnd) }.as_bool() {
            found.push(hwnd);
        }
        BOOL::from(found.len() < 6)
    }

    fn hidden_windows() -> Vec<HWND> {
        let mut found: Vec<HWND> = Vec::new();
        let _ = unsafe { EnumWindows(Some(collect_hidden), LPARAM(&raw mut found as isize)) };
        found
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
