use std::{ffi::c_void, mem::size_of, ptr};

use deckhand_core::{Error, Rect, Result, WindowRef, WindowState};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::{
        GA_ROOT, GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW,
        GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
        WINDOW_EX_STYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    },
};

use crate::frame::rect_from_win32;

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

pub(crate) fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

pub(crate) fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let read = unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, ptr::from_mut(&mut cloaked).cast(), size_of::<u32>() as u32)
    };
    read.is_ok() && cloaked != 0
}

pub(crate) fn is_tool_window(hwnd: HWND) -> bool {
    let style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32);
    style.contains(WS_EX_TOOLWINDOW) && !style.contains(WS_EX_APPWINDOW)
}

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

pub(crate) fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) }.as_bool()
}

pub(crate) fn is_maximized(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd) }.as_bool()
}

pub(crate) fn show_state(hwnd: HWND) -> WindowState {
    if is_minimized(hwnd) {
        WindowState::Minimized
    } else if is_maximized(hwnd) {
        WindowState::Maximized
    } else {
        WindowState::Normal
    }
}

pub(crate) fn root_owner(hwnd: HWND) -> HWND {
    unsafe { GetAncestor(hwnd, GA_ROOTOWNER) }
}

pub(crate) fn is_top_level(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd
}

pub(crate) fn current_foreground() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Foreground {
    window: Option<HWND>,
    root_owner: Option<HWND>,
}

impl Foreground {
    pub(crate) fn now() -> Self {
        let window = current_foreground();
        let root_owner = window.map(root_owner).filter(|owner| !owner.is_invalid());
        Self { window, root_owner }
    }

    pub(crate) fn includes(self, hwnd: HWND) -> bool {
        self.window == Some(hwnd) || self.root_owner == Some(hwnd)
    }
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
    }
}
