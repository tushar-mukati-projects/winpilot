use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, RECT};
use winpilot_core::{Rect, WindowRef};

pub(crate) const fn rect_from_win32(rect: RECT) -> Rect {
    Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
}

pub(crate) fn hwnd_from_ref(window: WindowRef) -> HWND {
    HWND(window.0 as isize as *mut c_void)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_win32_rects_and_handles() {
        let rect = rect_from_win32(RECT { left: -1920, top: 10, right: 0, bottom: 1090 });
        assert_eq!(rect, Rect::new(-1920, 10, 1920, 1080));
        assert_eq!(hwnd_from_ref(WindowRef(0x1234)).0 as isize, 0x1234);
    }
}
