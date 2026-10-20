use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MENU};
use winpilot_win32::{marked_key, send_input};

const UNASSIGNED_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
const KEY_DOWN: KEYBD_EVENT_FLAGS = KEYBD_EVENT_FLAGS(0);
const NO_SCAN_CODE: u16 = 0;

pub(crate) fn send_foreground_unlock() {
    if send_input(&unlock_inputs()).is_err() {
        let _ = send_input(&release_inputs());
    }
}

fn unlock_inputs() -> [INPUT; 4] {
    [
        marked_key(VK_MENU, NO_SCAN_CODE, KEY_DOWN),
        marked_key(UNASSIGNED_KEY, NO_SCAN_CODE, KEY_DOWN),
        marked_key(UNASSIGNED_KEY, NO_SCAN_CODE, KEYEVENTF_KEYUP),
        marked_key(VK_MENU, NO_SCAN_CODE, KEYEVENTF_KEYUP),
    ]
}

fn release_inputs() -> [INPUT; 2] {
    [marked_key(UNASSIGNED_KEY, NO_SCAN_CODE, KEYEVENTF_KEYUP), marked_key(VK_MENU, NO_SCAN_CODE, KEYEVENTF_KEYUP)]
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD;
    use winpilot_core::INJECTION_MARK;

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
