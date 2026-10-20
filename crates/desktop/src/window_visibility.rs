use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{IsZoomed, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW},
};
use winpilot_core::WindowState;
use winpilot_win32::{is_minimized, window_ex_style};

pub(crate) fn is_tool_window(hwnd: HWND) -> bool {
    let style = window_ex_style(hwnd);
    style.contains(WS_EX_TOOLWINDOW) && !style.contains(WS_EX_APPWINDOW)
}

fn is_maximized(hwnd: HWND) -> bool {
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
    fn null_window_reads_as_normal() {
        assert_eq!(show_state(HWND::default()), WindowState::Normal);
    }
}
