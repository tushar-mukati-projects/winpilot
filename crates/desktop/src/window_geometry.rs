use std::{mem::size_of, ptr};

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::GetWindowRect,
};
use winpilot_core::Rect;

use crate::frame::rect_from_win32;

pub(crate) fn outer_rect(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &raw mut rect) }.ok()?;
    Some(rect_from_win32(rect))
}

pub(crate) fn frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            ptr::from_mut(&mut rect).cast(),
            size_of::<RECT>() as u32,
        )
    };
    read.ok()?;
    Some(rect_from_win32(rect))
}

pub(crate) fn visible_rect(hwnd: HWND) -> Option<Rect> {
    frame_bounds(hwnd).or_else(|| outer_rect(hwnd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_window_has_no_rect() {
        assert!(visible_rect(HWND::default()).is_none());
    }
}
