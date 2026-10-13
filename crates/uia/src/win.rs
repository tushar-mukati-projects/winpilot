use std::ffi::c_void;

use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
        WindowFromPhysicalPoint,
    },
};
use winpilot_core::{Point, WindowRef};

const MAX_CLASS_NAME: usize = 256;

pub(crate) fn hwnd(window: WindowRef) -> HWND {
    HWND(window.0 as isize as *mut c_void)
}

pub(crate) fn window_ref(handle: HWND) -> WindowRef {
    WindowRef(handle.0 as isize as i64)
}

pub(crate) fn window_exists(window: WindowRef) -> bool {
    window.0 != 0 && unsafe { IsWindow(Some(hwnd(window))) }.as_bool()
}

pub(crate) fn is_minimized(window: WindowRef) -> bool {
    window.0 != 0 && unsafe { IsIconic(hwnd(window)) }.as_bool()
}

pub(crate) fn process_id(window: WindowRef) -> Option<u32> {
    if window.0 == 0 {
        return None;
    }
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd(window), Some(&raw mut pid)) };
    (thread != 0).then_some(pid)
}

pub(crate) fn class_name(window: WindowRef) -> Option<String> {
    if window.0 == 0 {
        return None;
    }
    let mut buffer = [0u16; MAX_CLASS_NAME];
    let length = unsafe { GetClassNameW(hwnd(window), &mut buffer) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(String::from_utf16_lossy(buffer.get(..length)?))
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

pub(crate) fn is_foreground(window: WindowRef) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if window.0 == 0 || foreground.is_invalid() {
        return false;
    }
    window_ref(foreground) == window || root_window(foreground) == Some(window)
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
    use crate::test_support::taskbar;

    #[test]
    fn window_refs_round_trip_through_hwnd() {
        let window = WindowRef(0x0012_3456);
        assert_eq!(window_ref(hwnd(window)), window);
    }

    #[test]
    fn null_window_does_not_exist() {
        assert!(!window_exists(WindowRef(0)));
        assert_eq!(root_of(WindowRef(0)), None);
        assert!(!is_foreground(WindowRef(0)));
        assert!(!is_minimized(WindowRef(0)));
        assert_eq!(process_id(WindowRef(0)), None);
        assert_eq!(class_name(WindowRef(0)), None);
    }

    #[test]
    fn taskbar_has_a_process_and_class() {
        let window = taskbar().hwnd;
        assert!(process_id(window).is_some_and(|pid| pid > 0));
        assert_eq!(class_name(window).as_deref(), Some("Shell_TrayWnd"));
    }
}
