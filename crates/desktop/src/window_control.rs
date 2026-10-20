use std::time::Duration;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, SMTO_ABORTIFHUNG, ShowWindowAsync, WM_CLOSE, WM_NULL},
};
use winpilot_core::{Context, Result, WindowRef, WindowState};
use winpilot_win32::{finite_millis, hwnd_exists, hwnd_of, live_hwnd, send_message_timeout};

use crate::activation::Activation;

pub fn set_window_state(hwnd: WindowRef, state: WindowState, activation: Activation) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    if unsafe { ShowWindowAsync(handle, activation.state_command(state)) }.as_bool() {
        return Ok(());
    }
    Err(windows::core::Error::from_thread()).context(&format!("changing the state of {hwnd}"))
}

pub fn close_window(hwnd: WindowRef) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    unsafe { PostMessageW(Some(handle), WM_CLOSE, WPARAM(0), LPARAM(0)) }.context(&format!("asking {hwnd} to close"))
}

pub fn wait_until_responsive(hwnd: WindowRef, timeout: Duration) -> bool {
    let handle = hwnd_of(hwnd);
    if !hwnd_exists(handle) {
        return false;
    }
    send_message_timeout(handle, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, finite_millis(timeout)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::foreground_window;

    #[test]
    #[ignore = "reads the user's windows"]
    fn foreground_window_is_responsive() {
        let foreground = foreground_window().unwrap();
        assert!(wait_until_responsive(foreground, Duration::from_secs(2)));
    }

    #[test]
    fn missing_windows_are_rejected() {
        assert!(!wait_until_responsive(WindowRef(0), Duration::from_millis(10)));
        assert!(close_window(WindowRef(0)).is_err());
        assert!(set_window_state(WindowRef(0), WindowState::Normal, Activation::Activate).is_err());
    }

    #[test]
    #[ignore = "minimizes and restores a real window"]
    fn minimize_and_restore_foreground_window() {
        let foreground = foreground_window().unwrap();
        set_window_state(foreground, WindowState::Minimized, Activation::Activate).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        set_window_state(foreground, WindowState::Normal, Activation::Activate).unwrap();
    }

    #[test]
    #[ignore = "closes a real window"]
    fn close_window_posts_close() {
        let foreground = foreground_window().unwrap();
        close_window(foreground).unwrap();
    }
}
