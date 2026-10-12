use std::{mem::size_of, ptr};

use deckhand_core::INJECTION_MARK;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

use crate::{
    chord_detector::PhysicalKey,
    input_clock::{record_user_input, record_user_press},
    physical_hotkey,
};

const READ_FAILED: u32 = u32::MAX;
const MOUSE: u32 = RIM_TYPEMOUSE.0;
const KEYBOARD: u32 = RIM_TYPEKEYBOARD.0;
const BUTTON_DOWN_FLAGS: u16 = 0x0001 | 0x0004 | 0x0010 | 0x0040 | 0x0100;

pub(crate) fn handle_raw_input(handle: HRAWINPUT) {
    let Some(input) = read_raw_input(handle) else { return };
    if counts_as_user(&input) {
        record_user_input();
        if presses_a_mouse_button(&input) {
            record_user_press();
        }
    }
    if let Some(key) = physical_key(&input) {
        physical_hotkey::observe(key);
    }
}

fn physical_key(input: &RAWINPUT) -> Option<PhysicalKey> {
    if input.header.dwType != KEYBOARD || input.header.hDevice.0.is_null() {
        return None;
    }
    let keyboard = unsafe { input.data.keyboard };
    if is_injection_mark(keyboard.ExtraInformation) {
        return None;
    }
    Some(PhysicalKey { make_code: keyboard.MakeCode, flags: keyboard.Flags, vk: keyboard.VKey })
}

fn presses_a_mouse_button(input: &RAWINPUT) -> bool {
    if input.header.dwType != MOUSE {
        return false;
    }
    let flags = unsafe { input.data.mouse.Anonymous.Anonymous.usButtonFlags };
    flags & BUTTON_DOWN_FLAGS != 0
}

fn read_raw_input(handle: HRAWINPUT) -> Option<RAWINPUT> {
    let mut input = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    let buffer = Some(ptr::from_mut(&mut input).cast());
    let copied =
        unsafe { GetRawInputData(handle, RID_INPUT, buffer, &raw mut size, size_of::<RAWINPUTHEADER>() as u32) };
    (copied != READ_FAILED && copied != 0).then_some(input)
}

fn counts_as_user(input: &RAWINPUT) -> bool {
    let Some(extra_info) = extra_information(input) else {
        return false;
    };
    is_user_input(!input.header.hDevice.0.is_null(), extra_info)
}

fn extra_information(input: &RAWINPUT) -> Option<u32> {
    match input.header.dwType {
        MOUSE => Some(unsafe { input.data.mouse.ulExtraInformation }),
        KEYBOARD => Some(unsafe { input.data.keyboard.ExtraInformation }),
        _ => None,
    }
}

fn is_user_input(from_a_device: bool, extra_info: u32) -> bool {
    from_a_device || !is_injection_mark(extra_info)
}

fn is_injection_mark(extra_info: u32) -> bool {
    usize::try_from(extra_info).is_ok_and(|extra| extra == INJECTION_MARK)
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use windows::Win32::{
        Foundation::HANDLE,
        UI::Input::{RAWKEYBOARD, RAWMOUSE, RIM_TYPEHID},
    };

    use super::*;

    const MARK: u32 = INJECTION_MARK as u32;
    const SOME_DEVICE: HANDLE = HANDLE(0x1234 as *mut c_void);

    fn mouse_input(device: HANDLE, extra: u32) -> RAWINPUT {
        let mut input = RAWINPUT::default();
        input.header.dwType = RIM_TYPEMOUSE.0;
        input.header.hDevice = device;
        input.data.mouse = RAWMOUSE { ulExtraInformation: extra, ..RAWMOUSE::default() };
        input
    }

    fn keyboard_input(device: HANDLE, extra: u32) -> RAWINPUT {
        let mut input = RAWINPUT::default();
        input.header.dwType = RIM_TYPEKEYBOARD.0;
        input.header.hDevice = device;
        input.data.keyboard = RAWKEYBOARD { ExtraInformation: extra, ..RAWKEYBOARD::default() };
        input
    }

    #[test]
    fn physical_devices_count_as_the_user() {
        assert!(counts_as_user(&mouse_input(SOME_DEVICE, 0)));
        assert!(counts_as_user(&keyboard_input(SOME_DEVICE, MARK)));
    }

    #[test]
    fn other_injectors_count_as_the_user() {
        assert!(counts_as_user(&mouse_input(HANDLE::default(), 0)));
        assert!(counts_as_user(&keyboard_input(HANDLE::default(), 0x1234)));
    }

    #[test]
    fn our_own_marked_injection_is_ignored() {
        assert!(!counts_as_user(&mouse_input(HANDLE::default(), MARK)));
        assert!(!counts_as_user(&keyboard_input(HANDLE::default(), MARK)));
    }

    #[test]
    fn other_device_types_are_ignored() {
        let mut input = keyboard_input(SOME_DEVICE, 0);
        input.header.dwType = RIM_TYPEHID.0;
        assert!(!counts_as_user(&input));
    }

    #[test]
    fn only_unmarked_keys_from_a_real_keyboard_reach_the_kill_chord() {
        let mut real = keyboard_input(SOME_DEVICE, 0);
        real.data.keyboard.VKey = 0x1B;
        assert_eq!(physical_key(&real).map(|key| key.vk), Some(0x1B));
        assert_eq!(physical_key(&keyboard_input(HANDLE::default(), 0)), None);
        assert_eq!(physical_key(&keyboard_input(SOME_DEVICE, MARK)), None);
        assert_eq!(physical_key(&mouse_input(SOME_DEVICE, 0)), None);
    }

    #[test]
    fn only_mouse_button_downs_count_as_presses() {
        let mut moved = mouse_input(SOME_DEVICE, 0);
        assert!(!presses_a_mouse_button(&moved));
        for down in [0x0001, 0x0004, 0x0010, 0x0040, 0x0100] {
            moved.data.mouse.Anonymous.Anonymous.usButtonFlags = down;
            assert!(presses_a_mouse_button(&moved), "{down:#x}");
        }
        for up_or_wheel in [0x0002, 0x0008, 0x0020, 0x0080, 0x0200, 0x0400, 0x0800] {
            moved.data.mouse.Anonymous.Anonymous.usButtonFlags = up_or_wheel;
            assert!(!presses_a_mouse_button(&moved), "{up_or_wheel:#x}");
        }
        assert!(!presses_a_mouse_button(&keyboard_input(SOME_DEVICE, 0)));
    }

    #[test]
    fn the_mark_fits_the_raw_input_field() {
        assert_eq!(usize::try_from(MARK).unwrap(), INJECTION_MARK);
    }
}
