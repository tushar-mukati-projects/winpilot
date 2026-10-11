use deckhand_core::keys::Modifiers;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        GetKeyboardLayout, HKL, MAPVK_VK_TO_CHAR, MAPVK_VK_TO_VSC_EX, MapVirtualKeyExW, VIRTUAL_KEY, VkKeyScanExW,
    },
    WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
};

const SHIFT_STATE_SHIFT: u8 = 1;
const SHIFT_STATE_CTRL: u8 = 2;
const SHIFT_STATE_ALT: u8 = 4;
const REPRODUCIBLE_SHIFT_STATES: u8 = SHIFT_STATE_SHIFT | SHIFT_STATE_CTRL | SHIFT_STATE_ALT;
const NO_KEY: u8 = 0xFF;
const CURRENT_THREAD: u32 = 0;
const DEAD_KEY_BIT: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutKey {
    pub vk: VIRTUAL_KEY,
    pub modifiers: Modifiers,
}

pub trait KeyboardLayout {
    fn char_key(&self, c: char) -> Option<LayoutKey>;
    fn scan_code(&self, vk: VIRTUAL_KEY) -> u32;
    fn is_dead_key(&self, vk: VIRTUAL_KEY) -> bool;
}

pub struct SystemLayout {
    handle: HKL,
}

impl SystemLayout {
    pub fn of_foreground_window() -> Self {
        let foreground = thread_layout(foreground_thread_id());
        let handle = if foreground.is_invalid() { thread_layout(CURRENT_THREAD) } else { foreground };
        Self { handle }
    }
}

impl KeyboardLayout for SystemLayout {
    fn char_key(&self, c: char) -> Option<LayoutKey> {
        let unit = u16::try_from(u32::from(c)).ok()?;
        decode_vk_scan(unsafe { VkKeyScanExW(unit, self.handle) })
    }

    fn scan_code(&self, vk: VIRTUAL_KEY) -> u32 {
        unsafe { MapVirtualKeyExW(u32::from(vk.0), MAPVK_VK_TO_VSC_EX, Some(self.handle)) }
    }

    fn is_dead_key(&self, vk: VIRTUAL_KEY) -> bool {
        let character = unsafe { MapVirtualKeyExW(u32::from(vk.0), MAPVK_VK_TO_CHAR, Some(self.handle)) };
        marks_dead_key(character)
    }
}

fn foreground_thread_id() -> u32 {
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return CURRENT_THREAD;
    }
    unsafe { GetWindowThreadProcessId(window, None) }
}

fn thread_layout(thread_id: u32) -> HKL {
    unsafe { GetKeyboardLayout(thread_id) }
}

fn marks_dead_key(character: u32) -> bool {
    character & DEAD_KEY_BIT != 0
}

pub fn decode_vk_scan(result: i16) -> Option<LayoutKey> {
    let [vk, state] = result.to_le_bytes();
    if vk == NO_KEY || state & !REPRODUCIBLE_SHIFT_STATES != 0 {
        return None;
    }
    let modifiers = Modifiers {
        ctrl: state & SHIFT_STATE_CTRL != 0,
        alt: state & SHIFT_STATE_ALT != 0,
        shift: state & SHIFT_STATE_SHIFT != 0,
        win: false,
    };
    Some(LayoutKey { vk: VIRTUAL_KEY(u16::from(vk)), modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minus_one_means_not_on_the_layout() {
        assert_eq!(decode_vk_scan(-1), None);
    }

    #[test]
    fn plain_letter_needs_no_modifiers() {
        let key = decode_vk_scan(0x0041).unwrap();
        assert_eq!(key, LayoutKey { vk: VIRTUAL_KEY(0x41), modifiers: Modifiers::NONE });
    }

    #[test]
    fn capital_letter_needs_shift() {
        let key = decode_vk_scan(0x0141).unwrap();
        assert_eq!(key.vk, VIRTUAL_KEY(0x41));
        assert_eq!(key.modifiers, Modifiers { shift: true, ..Modifiers::NONE });
    }

    #[test]
    fn altgr_character_needs_ctrl_and_alt() {
        let key = decode_vk_scan(0x0651).unwrap();
        assert_eq!(key.vk, VIRTUAL_KEY(0x51));
        assert_eq!(key.modifiers, Modifiers { ctrl: true, alt: true, ..Modifiers::NONE });
    }

    #[test]
    fn only_the_top_bit_marks_a_dead_key() {
        assert!(marks_dead_key(0x8000_005E));
        assert!(!marks_dead_key(0x5E));
        assert!(!marks_dead_key(0x8B5E));
        assert!(!marks_dead_key(0));
    }

    #[test]
    fn hankaku_and_reserved_states_are_not_reproducible() {
        assert_eq!(decode_vk_scan(0x0841), None);
        assert_eq!(decode_vk_scan(0x1041), None);
    }
}
