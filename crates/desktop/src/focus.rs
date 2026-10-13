use std::time::{Duration, Instant};

use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
use winpilot_core::{Error, ErrorCode, Result, WindowRef};

use crate::{
    foreground_unlock::send_foreground_unlock,
    process::runs_above_own_integrity,
    timeouts::{later, poll_until},
    window_foreground::Foreground,
    window_identity::{live_hwnd, owning_pid},
    window_restore::unminimize,
    window_visibility::is_minimized,
};

const RETRIES: u32 = 2;
const ATTEMPT_WINDOW: Duration = Duration::from_millis(30);
const FOREGROUND_POLL: Duration = Duration::from_millis(5);

pub fn focus_window(hwnd: WindowRef) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    if is_minimized(handle) {
        unminimize(handle, hwnd)?;
    }
    if attempt(handle, request_foreground) {
        return Ok(());
    }
    let retry = retry_request(handle);
    for _ in 0..RETRIES {
        if attempt(handle, retry) {
            return Ok(());
        }
    }
    Err(Error::new(ErrorCode::NeedsUser, format!("Windows refused to bring {hwnd} to the foreground")))
}

fn attempt(hwnd: HWND, request: fn(HWND)) -> bool {
    request(hwnd);
    wait_for_foreground(hwnd, ATTEMPT_WINDOW)
}

fn retry_request(hwnd: HWND) -> fn(HWND) {
    if unlock_allowed(hwnd) { request_foreground_after_unlock } else { request_foreground }
}

fn unlock_allowed(hwnd: HWND) -> bool {
    !runs_above_own_integrity(owning_pid(hwnd))
}

fn request_foreground(hwnd: HWND) {
    let _accepted = unsafe { SetForegroundWindow(hwnd) };
}

fn request_foreground_after_unlock(hwnd: HWND) {
    send_foreground_unlock();
    request_foreground(hwnd);
}

fn wait_for_foreground(hwnd: HWND, within: Duration) -> bool {
    poll_until(later(Instant::now(), within), FOREGROUND_POLL, || Foreground::now().includes(hwnd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::foreground_window;

    #[test]
    fn focusing_a_missing_window_is_not_found() {
        let error = focus_window(WindowRef(0)).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn unlock_is_skipped_when_the_target_process_cannot_be_inspected() {
        assert!(!unlock_allowed(HWND::default()));
    }

    #[test]
    #[ignore = "changes focus"]
    fn focus_the_window_below_the_foreground_and_back() {
        let original = foreground_window().unwrap();
        let other = crate::window_list::windows().unwrap().into_iter().find(|info| !info.foreground).unwrap();
        focus_window(other.hwnd).unwrap();
        focus_window(original).unwrap();
    }
}
