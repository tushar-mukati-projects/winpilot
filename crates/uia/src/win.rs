use std::ffi::c_void;

use deckhand_core::{Point, WindowRef};
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetForegroundWindow, IsWindow, WindowFromPhysicalPoint},
};

pub(crate) fn hwnd(window: WindowRef) -> HWND {
    HWND(window.0 as isize as *mut c_void)
}

pub(crate) fn window_ref(handle: HWND) -> WindowRef {
    WindowRef(handle.0 as isize as i64)
}

pub(crate) fn window_exists(window: WindowRef) -> bool {
    window.0 != 0 && unsafe { IsWindow(Some(hwnd(window))) }.as_bool()
}

pub(crate) fn root_of(window: WindowRef) -> Option<WindowRef> {
    if window.0 == 0 {
        return None;
    }
    root_window(hwnd(window))
}

pub(crate) fn window_at(point: Point) -> Option<WindowRef> {
    root_window(unsafe { WindowFromPhysicalPoint(POINT { x: point.x, y: point.y }) })
}

pub(crate) fn foreground_window() -> Option<WindowRef> {
    root_window(unsafe { GetForegroundWindow() })
}

fn root_window(handle: HWND) -> Option<WindowRef> {
    if handle.is_invalid() {
        return None;
    }
    let root = unsafe { GetAncestor(handle, GA_ROOT) };
    (!root.is_invalid()).then(|| window_ref(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_refs_round_trip_through_hwnd() {
        let window = WindowRef(0x0012_3456);
        assert_eq!(window_ref(hwnd(window)), window);
    }

    #[test]
    fn null_window_does_not_exist() {
        assert!(!window_exists(WindowRef(0)));
        assert_eq!(root_of(WindowRef(0)), None);
    }
}
