use std::thread;

use windows::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, ERROR_TIMEOUT, GetLastError, HWND, LPARAM, WIN32_ERROR, WPARAM},
    UI::WindowsAndMessaging::SMTO_ABORTIFHUNG,
};
use winpilot_core::{Error, ErrorCode, Result, WindowRef};
use winpilot_win32::{ref_of, send_message_timeout};

use crate::message_plan::Sent;

const ANSWER_TIMEOUT_MS: u32 = 1000;

pub(crate) fn deliver(hwnd: HWND, plan: &[Sent], mut check: impl FnMut() -> Result<()>) -> Result<()> {
    for sent in plan {
        check()?;
        match *sent {
            Sent::Pause(duration) => thread::sleep(duration),
            Sent::Message { id, wparam, lparam } => send_one(hwnd, id, wparam, lparam)?,
        }
    }
    Ok(())
}

pub(crate) fn deliver_releasing(
    hwnd: HWND,
    plan: &[Sent],
    release: Sent,
    check: impl FnMut() -> Result<()>,
) -> Result<()> {
    let outcome = deliver(hwnd, plan, check);
    if outcome.is_err() {
        let _ = deliver(hwnd, &[release], || Ok(()));
    }
    outcome
}

fn send_one(hwnd: HWND, id: u32, wparam: usize, lparam: isize) -> Result<()> {
    if send_message_timeout(hwnd, id, WPARAM(wparam), LPARAM(lparam), SMTO_ABORTIFHUNG, ANSWER_TIMEOUT_MS).is_some() {
        return Ok(());
    }
    Err(unanswered(last_error(), ref_of(hwnd), id))
}

fn last_error() -> WIN32_ERROR {
    unsafe { GetLastError() }
}

fn unanswered(error: WIN32_ERROR, window: WindowRef, id: u32) -> Error {
    match error {
        ERROR_ACCESS_DENIED => Error::new(
            ErrorCode::ElevatedTarget,
            format!("Windows blocked window messages to {window} (it runs at a higher integrity level)"),
        ),
        ERROR_TIMEOUT => Error::target_not_responding(format!(
            "{window} did not answer a window message within a second; it may be busy or hung"
        )),
        other => {
            Error::internal(format!("sending window message 0x{id:04X} to {window} failed (Win32 error {})", other.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::ERROR_INVALID_WINDOW_HANDLE;

    use super::*;

    #[test]
    fn failures_say_whether_the_window_was_blocked_hung_or_gone() {
        let blocked = unanswered(ERROR_ACCESS_DENIED, WindowRef(7), 0x0201);
        assert_eq!(blocked.code, ErrorCode::ElevatedTarget);
        let hung = unanswered(ERROR_TIMEOUT, WindowRef(7), 0x0201);
        assert_eq!(hung.code, ErrorCode::TargetNotResponding);
        assert!(hung.message.starts_with("w7 did not answer"), "{}", hung.message);
        let gone = unanswered(ERROR_INVALID_WINDOW_HANDLE, WindowRef(7), 0x0201);
        assert_eq!(gone.code, ErrorCode::Internal);
        assert!(gone.message.contains("0x0201"), "{}", gone.message);
    }

    #[test]
    fn a_failed_check_stops_before_anything_is_sent() {
        let plan = [Sent::Message { id: 0x0201, wparam: 1, lparam: 0 }];
        let stopped = deliver(HWND::default(), &plan, || Err(Error::new(ErrorCode::StoppedByUser, "stopped")));
        assert_eq!(stopped.unwrap_err().code, ErrorCode::StoppedByUser);
        assert!(deliver(HWND::default(), &[], || Ok(())).is_ok());
    }
}
