use deckhand_core::WindowRef;
use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, WINDOW_EX_STYLE, WS_EX_TOPMOST};

use crate::window_identity::{hwnd_of, is_live};

pub fn is_topmost(hwnd: WindowRef) -> bool {
    let handle = hwnd_of(hwnd);
    if !is_live(handle) {
        return false;
    }
    let style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } as u32);
    style.contains(WS_EX_TOPMOST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_window_is_not_topmost() {
        assert!(!is_topmost(WindowRef(0)));
    }

    #[test]
    fn most_app_windows_are_not_topmost() {
        let windows = crate::window_list::windows().unwrap();
        let topmost = windows.iter().filter(|window| is_topmost(window.hwnd)).count();
        assert!(topmost < windows.len(), "every app window claims to be topmost");
    }
}
