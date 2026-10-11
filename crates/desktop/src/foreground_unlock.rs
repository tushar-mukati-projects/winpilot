use std::mem::size_of;

use deckhand_core::INJECTION_MARK;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_MENU,
};

const UNASSIGNED_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
const KEY_DOWN: KEYBD_EVENT_FLAGS = KEYBD_EVENT_FLAGS(0);

pub(crate) fn send_foreground_unlock() {
    let unlock = unlock_inputs();
    if send_marked(&unlock) != unlock.len() {
        let _ = send_marked(&release_inputs());
    }
}

fn unlock_inputs() -> [INPUT; 4] {
    [
        marked_key(VK_MENU, KEY_DOWN),
        marked_key(UNASSIGNED_KEY, KEY_DOWN),
        marked_key(UNASSIGNED_KEY, KEYEVENTF_KEYUP),
        marked_key(VK_MENU, KEYEVENTF_KEYUP),
    ]
}

fn release_inputs() -> [INPUT; 2] {
    [marked_key(UNASSIGNED_KEY, KEYEVENTF_KEYUP), marked_key(VK_MENU, KEYEVENTF_KEYUP)]
}

fn marked_key(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    let ki = KEYBDINPUT { wVk: key, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: INJECTION_MARK };
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }
}

fn send_marked(inputs: &[INPUT]) -> usize {
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) as usize }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(inputs: &[INPUT]) -> Vec<(VIRTUAL_KEY, KEYBD_EVENT_FLAGS, usize)> {
        inputs
            .iter()
            .map(|input| {
                assert_eq!(input.r#type, INPUT_KEYBOARD);
                let ki = unsafe { input.Anonymous.ki };
                (ki.wVk, ki.dwFlags, ki.dwExtraInfo)
            })
            .collect()
    }

    #[test]
    fn unlock_taps_an_unassigned_key_inside_one_alt_press() {
        assert_eq!(
            keys(&unlock_inputs()),
            vec![
                (VK_MENU, KEY_DOWN, INJECTION_MARK),
                (UNASSIGNED_KEY, KEY_DOWN, INJECTION_MARK),
                (UNASSIGNED_KEY, KEYEVENTF_KEYUP, INJECTION_MARK),
                (VK_MENU, KEYEVENTF_KEYUP, INJECTION_MARK),
            ]
        );
    }

    #[test]
    fn partial_unlock_releases_both_keys() {
        assert_eq!(
            keys(&release_inputs()),
            vec![(UNASSIGNED_KEY, KEYEVENTF_KEYUP, INJECTION_MARK), (VK_MENU, KEYEVENTF_KEYUP, INJECTION_MARK)]
        );
    }
}
