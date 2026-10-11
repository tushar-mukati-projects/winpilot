use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, IsDialogMessageW, MSG, TranslateMessage};
use windows::core::{Error, Result};

use crate::shortcuts::Shortcuts;

pub fn run(window: HWND, shortcuts: &Shortcuts) -> Result<i32> {
    let mut message = MSG::default();
    loop {
        match unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 {
            -1 => return Err(Error::from_thread()),
            0 => return Ok(message.wParam.0 as i32),
            _ => route(window, shortcuts, &message),
        }
    }
}

fn route(window: HWND, shortcuts: &Shortcuts, message: &MSG) {
    if shortcuts.translate(window, message) {
        return;
    }
    if unsafe { IsDialogMessageW(window, message) }.as_bool() {
        return;
    }
    let _produced_character = unsafe { TranslateMessage(message) };
    let _window_result = unsafe { DispatchMessageW(message) };
}
