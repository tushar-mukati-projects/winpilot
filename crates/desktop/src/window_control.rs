use std::time::Duration;

use deckhand_core::{Context, Result, WindowRef, WindowState};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        PostMessageW, SMTO_ABORTIFHUNG, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SendMessageTimeoutW, ShowWindowAsync,
        WM_CLOSE, WM_NULL,
    },
};

use crate::{
    timeouts::finite_millis,
    window_identity::{hwnd_of, is_live, live_hwnd},
};

pub fn set_window_state(hwnd: WindowRef, state: WindowState) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    let command = match state {
        WindowState::Minimized => SW_MINIMIZE,
        WindowState::Maximized => SW_MAXIMIZE,
        WindowState::Normal => SW_RESTORE,
    };
    if unsafe { ShowWindowAsync(handle, command) }.as_bool() {
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
        assert!(set_window_state(WindowRef(0), WindowState::Normal).is_err());
    }

    #[test]
    #[ignore = "minimizes and restores a real window"]
    fn minimize_and_restore_foreground_window() {
        let foreground = foreground_window().unwrap();
        set_window_state(foreground, WindowState::Minimized).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        set_window_state(foreground, WindowState::Normal).unwrap();
    }

    #[test]
    #[ignore = "closes a real window"]
    fn close_window_posts_close() {
        let foreground = foreground_window().unwrap();
        close_window(foreground).unwrap();
    }
}
