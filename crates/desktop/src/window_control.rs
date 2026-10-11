use std::time::Duration;

use deckhand_core::{Context, Error, ErrorCode, Rect, Result, WindowRef, WindowState};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        IsHungAppWindow, PostMessageW, SMTO_ABORTIFHUNG, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SWP_ASYNCWINDOWPOS,
        SWP_NOACTIVATE, SWP_NOZORDER, SendMessageTimeoutW, SetWindowPos, ShowWindow, ShowWindowAsync, WM_CLOSE,
        WM_NULL,
    },
};

use crate::{
    frame::FrameInsets,
    timeouts::finite_millis,
    window_facts::{frame_bounds, hwnd_of, is_live, is_maximized, live_hwnd, outer_rect},
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

pub fn move_window(hwnd: WindowRef, rect: Rect) -> Result<()> {
    if rect.is_empty() {
        return Err(Error::invalid("a window needs a positive width and height"));
    }
    let handle = live_hwnd(hwnd)?;
    if is_maximized(handle) {
        restore_window(handle, hwnd)?;
    }
    let placement = outer_placement(handle, rect);
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    unsafe { SetWindowPos(handle, None, placement.x, placement.y, placement.width, placement.height, flags) }
        .context(&format!("moving {hwnd}"))
}

fn outer_placement(hwnd: HWND, visible_target: Rect) -> Rect {
    let insets = match (outer_rect(hwnd), frame_bounds(hwnd)) {
        (Some(outer), Some(visible)) => FrameInsets::between(outer, visible),
        _ => FrameInsets::default(),
    };
    insets.outer_for(visible_target)
}

pub(crate) fn restore_window(hwnd: HWND, window: WindowRef) -> Result<()> {
    if unsafe { IsHungAppWindow(hwnd) }.as_bool() {
        return Err(Error::new(ErrorCode::TargetNotResponding, format!("{window} is not responding")));
    }
    let _was_visible = unsafe { ShowWindow(hwnd, SW_RESTORE) };
    Ok(())
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

pub fn is_hung(hwnd: WindowRef) -> bool {
    let handle = hwnd_of(hwnd);
    is_live(handle) && unsafe { IsHungAppWindow(handle) }.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::foreground_window;

    #[test]
    fn foreground_window_is_responsive_and_not_hung() {
        let foreground = foreground_window().unwrap();
        assert!(!is_hung(foreground));
        assert!(wait_until_responsive(foreground, Duration::from_secs(2)));
    }

    #[test]
    fn missing_windows_are_rejected() {
        assert!(!wait_until_responsive(WindowRef(0), Duration::from_millis(10)));
        assert!(!is_hung(WindowRef(0)));
        assert!(close_window(WindowRef(0)).is_err());
        assert!(set_window_state(WindowRef(0), WindowState::Normal).is_err());
        assert!(move_window(WindowRef(0), Rect::new(0, 0, 10, 10)).is_err());
    }

    #[test]
    fn empty_target_rect_is_invalid() {
        let error = move_window(WindowRef(0), Rect::new(0, 0, 0, 10)).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    #[ignore = "moves a real window"]
    fn move_foreground_window_keeps_size() {
        let foreground = foreground_window().unwrap();
        let info = crate::window_list::window(foreground).unwrap();
        move_window(foreground, info.rect).unwrap();
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
