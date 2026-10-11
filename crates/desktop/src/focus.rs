use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Error, ErrorCode, Result, WindowRef};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};

use crate::{
    marked_input::AltHeld,
    window_control::restore_window,
    window_facts::{Foreground, is_minimized, live_hwnd},
};

const FOCUS_ATTEMPTS: u32 = 3;
const ATTEMPT_WINDOW: Duration = Duration::from_millis(30);
const FOREGROUND_POLL: Duration = Duration::from_millis(5);

pub fn focus_window(hwnd: WindowRef) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    if is_minimized(handle) {
        restore_window(handle, hwnd)?;
    }
    for attempt in 0..FOCUS_ATTEMPTS {
        request_foreground(handle, attempt > 0);
        if wait_for_foreground(handle, ATTEMPT_WINDOW) {
            return Ok(());
        }
    }
    Err(Error::new(ErrorCode::NeedsUser, format!("Windows refused to bring {hwnd} to the foreground")))
}

fn request_foreground(hwnd: HWND, with_alt_unlock: bool) {
    let _alt = if with_alt_unlock { AltHeld::press() } else { None };
    let _accepted = unsafe { SetForegroundWindow(hwnd) };
}

fn wait_for_foreground(hwnd: HWND, within: Duration) -> bool {
    let started = Instant::now();
    loop {
        if Foreground::now().includes(hwnd) {
            return true;
        }
        if started.elapsed() >= within {
            return false;
        }
        thread::sleep(FOREGROUND_POLL);
    }
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
    #[ignore = "changes focus"]
    fn focus_the_window_below_the_foreground_and_back() {
        let original = foreground_window().unwrap();
        let other = crate::window_list::windows().unwrap().into_iter().find(|info| !info.foreground).unwrap();
        focus_window(other.hwnd).unwrap();
        focus_window(original).unwrap();
    }
}
