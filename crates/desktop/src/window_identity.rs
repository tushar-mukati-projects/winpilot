use std::{ffi::c_void, ptr};

use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    },
};
use winpilot_core::{Error, Result, WindowRef};

const TITLE_CAPACITY: usize = 1024;
const CLASS_CAPACITY: usize = 256;

pub(crate) fn hwnd_of(window: WindowRef) -> HWND {
    HWND(window.0 as isize as *mut c_void)
}

pub(crate) fn ref_of(hwnd: HWND) -> WindowRef {
    WindowRef(hwnd.0 as isize as i64)
}

pub(crate) fn is_live(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

pub(crate) fn live_hwnd(window: WindowRef) -> Result<HWND> {
    let hwnd = hwnd_of(window);
    if is_live(hwnd) { Ok(hwnd) } else { Err(Error::not_found(format!("window {window} no longer exists"))) }
}

pub(crate) fn title(hwnd: HWND) -> String {
    let mut buffer = [0u16; TITLE_CAPACITY];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    utf16_prefix(&buffer, length)
}

pub(crate) fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; CLASS_CAPACITY];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    utf16_prefix(&buffer, length)
}

fn utf16_prefix(buffer: &[u16], length: i32) -> String {
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

pub(crate) fn owning_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(ptr::from_mut(&mut pid))) };
    if thread == 0 { 0 } else { pid }
}

pub(crate) fn is_top_level(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_refs_round_trip_through_hwnd() {
        let window = WindowRef(0x0002_04A6);
        assert_eq!(ref_of(hwnd_of(window)), window);
    }

    #[test]
    fn utf16_prefix_clamps_bad_lengths() {
        let buffer = [u16::from(b'a'), u16::from(b'b')];
        assert_eq!(utf16_prefix(&buffer, -1), "");
        assert_eq!(utf16_prefix(&buffer, 5), "ab");
    }

    #[test]
    fn null_window_is_not_live() {
        assert!(!is_live(HWND::default()));
        assert!(live_hwnd(WindowRef(0)).is_err());
        assert_eq!(owning_pid(HWND::default()), 0);
        assert!(!is_top_level(HWND::default()));
    }
}
