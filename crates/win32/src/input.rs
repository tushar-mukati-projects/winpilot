use std::mem::size_of;

use windows::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, GetLastError, WIN32_ERROR},
    UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, MOUSE_EVENT_FLAGS, MOUSEINPUT,
        SendInput, VIRTUAL_KEY,
    },
};
use winpilot_core::{Error, ErrorCode, INJECTION_MARK, Result};

const BLOCKED_BY_WINDOWS: &str =
    "input was blocked by Windows (the target window is elevated or the secure desktop is active)";

pub fn marked_key(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    let ki = KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_MARK };
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }
}

pub fn marked_mouse(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    let mi = MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_MARK };
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } }
}

pub fn send_input(inputs: &[INPUT]) -> Result<()> {
    let inserted = insert_inputs(inputs);
    if inserted as usize == inputs.len() {
        return Ok(());
    }
    Err(blocked_input_error(last_error(), inserted, inputs.len()))
}

fn insert_inputs(inputs: &[INPUT]) -> u32 {
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) }
}

fn last_error() -> WIN32_ERROR {
    unsafe { GetLastError() }
}

fn blocked_input_error(error: WIN32_ERROR, inserted: u32, expected: usize) -> Error {
    if error == ERROR_ACCESS_DENIED {
        return Error::new(ErrorCode::ElevatedTarget, BLOCKED_BY_WINDOWS);
    }
    Error::internal(format!("SendInput inserted {inserted} of {expected} input events (Win32 error {})", error.0))
}

#[cfg(test)]
mod tests {
    use windows::Win32::{
        Foundation::ERROR_INVALID_PARAMETER,
        UI::Input::KeyboardAndMouse::{KEYEVENTF_KEYUP, MOUSEEVENTF_MOVE, VK_MENU},
    };

    use super::*;

    #[test]
    fn keys_and_mouse_actions_carry_the_injection_mark() {
        let key = unsafe { marked_key(VK_MENU, 0x38, KEYEVENTF_KEYUP).Anonymous.ki };
        assert_eq!(
            (key.wVk, key.wScan, key.dwFlags, key.dwExtraInfo),
            (VK_MENU, 0x38, KEYEVENTF_KEYUP, INJECTION_MARK)
        );
        let mouse = unsafe { marked_mouse(1, -2, 3, MOUSEEVENTF_MOVE).Anonymous.mi };
        assert_eq!((mouse.dx, mouse.dy, mouse.mouseData, mouse.dwExtraInfo), (1, -2, 3, INJECTION_MARK));
    }

    #[test]
    fn access_denied_means_elevated_target() {
        let error = blocked_input_error(ERROR_ACCESS_DENIED, 0, 4);
        assert_eq!(error.code, ErrorCode::ElevatedTarget);
        assert_eq!(error.message, BLOCKED_BY_WINDOWS);
    }

    #[test]
    fn other_failures_are_internal() {
        let error = blocked_input_error(ERROR_INVALID_PARAMETER, 1, 4);
        assert_eq!(error.code, ErrorCode::Internal);
        assert!(error.message.contains("1 of 4"));
    }
}
