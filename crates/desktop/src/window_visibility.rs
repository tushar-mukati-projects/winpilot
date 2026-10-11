use std::{mem::size_of, ptr};

use deckhand_core::WindowState;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, IsIconic, IsWindowVisible, IsZoomed, WINDOW_EX_STYLE, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    },
};

pub(crate) fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

pub(crate) fn is_cloaked(hwnd: HWND) -> bool {
    cloak_flags(hwnd) != 0
}

pub(crate) fn cloak_flags(hwnd: HWND) -> u32 {
    let mut cloaked = 0u32;
    let read = unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, ptr::from_mut(&mut cloaked).cast(), size_of::<u32>() as u32)
    };
    if read.is_ok() { cloaked } else { 0 }
}

pub(crate) fn is_tool_window(hwnd: HWND) -> bool {
    let style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32);
    style.contains(WS_EX_TOOLWINDOW) && !style.contains(WS_EX_APPWINDOW)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_window_reads_as_hidden_and_normal() {
        assert!(!is_visible(HWND::default()));
        assert!(!is_cloaked(HWND::default()));
        assert_eq!(show_state(HWND::default()), WindowState::Normal);
    }
}
