use std::time::{Duration, Instant};

use deckhand_core::{Context, Error, ErrorCode, Result, WindowRef, WindowState};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{IsHungAppWindow, SW_RESTORE, ShowWindowAsync},
};

use crate::{
    timeouts::{later, poll_until},
    window_visibility::show_state,
};

const RESTORE_TIMEOUT: Duration = Duration::from_secs(1);
const RESTORE_POLL: Duration = Duration::from_millis(5);
const MAX_RESTORE_STEPS: usize = 2;

pub(crate) fn unminimize(hwnd: HWND, window: WindowRef) -> Result<()> {
    restore_until(hwnd, window, |state| state != WindowState::Minimized)
}

pub(crate) fn restore_to_normal(hwnd: HWND, window: WindowRef) -> Result<()> {
    restore_until(hwnd, window, |state| state == WindowState::Normal)
}

pub(crate) fn ensure_responsive(hwnd: HWND, window: WindowRef) -> Result<()> {
    if unsafe { IsHungAppWindow(hwnd) }.as_bool() {
        return Err(not_responding(window));
    }
    Ok(())
}

fn restore_until(hwnd: HWND, window: WindowRef, done: impl Fn(WindowState) -> bool) -> Result<()> {
    let deadline = later(Instant::now(), RESTORE_TIMEOUT);
    for _ in 0..MAX_RESTORE_STEPS {
        let before = show_state(hwnd);
        if done(before) {
            return Ok(());
        }
        ensure_responsive(hwnd, window)?;
        request_restore(hwnd, window)?;
        if !poll_until(deadline, RESTORE_POLL, || show_state(hwnd) != before) {
            return Err(not_responding(window));
        }
    }
    if done(show_state(hwnd)) { Ok(()) } else { Err(not_responding(window)) }
}

fn request_restore(hwnd: HWND, window: WindowRef) -> Result<()> {
    if unsafe { ShowWindowAsync(hwnd, SW_RESTORE) }.as_bool() {
        return Ok(());
    }
    Err(windows::core::Error::from_thread()).context(&format!("restoring {window}"))
}

fn not_responding(window: WindowRef) -> Error {
    Error::new(ErrorCode::TargetNotResponding, format!("{window} is not responding"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_window_needs_no_restore() {
        assert!(restore_to_normal(HWND::default(), WindowRef(0)).is_ok());
        assert!(unminimize(HWND::default(), WindowRef(0)).is_ok());
    }

    #[test]
    fn not_responding_names_the_window() {
        let error = not_responding(WindowRef(42));
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(error.message, "w42 is not responding");
    }
}
