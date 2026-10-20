use windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST;
use winpilot_core::WindowRef;
use winpilot_win32::{hwnd_exists, hwnd_of, window_ex_style};

pub fn is_topmost(hwnd: WindowRef) -> bool {
    let handle = hwnd_of(hwnd);
    if !hwnd_exists(handle) {
        return false;
    }
    window_ex_style(handle).contains(WS_EX_TOPMOST)
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
