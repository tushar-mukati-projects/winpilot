use std::mem::size_of;

use deckhand_core::INJECTION_MARK;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_MENU,
};

pub(crate) struct AltHeld {
    _private: (),
}

impl AltHeld {
    pub(crate) fn press() -> Option<Self> {
        send_marked_key(VK_MENU, KEYBD_EVENT_FLAGS(0)).then_some(Self { _private: () })
    }
}

impl Drop for AltHeld {
    fn drop(&mut self) {
        let _released = send_marked_key(VK_MENU, KEYEVENTF_KEYUP);
    }
}

fn send_marked_key(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> bool {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: key, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_MARK },
        },
    };
    let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    sent == 1
}
