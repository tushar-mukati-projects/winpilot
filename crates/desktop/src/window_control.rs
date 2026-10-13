use std::time::Duration;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, ShowWindowAsync, WM_CLOSE, WM_NULL,
    },
};
use winpilot_core::{Context, Result, WindowRef, WindowState};

use crate::{
    activation::Activation,
    timeouts::finite_millis,
    window_identity::{hwnd_of, is_live, live_hwnd},
};

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
    if !is_live(handle) {
        return false;
    }
    let mut reply = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            handle,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            finite_millis(timeout),
            Some(&raw mut reply),
        )
    };
    sent.0 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::foreground_window;

    #[test]
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
