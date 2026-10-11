use std::{mem::size_of, ptr};

use deckhand_core::INJECTION_MARK;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};

use crate::input_clock::record_user_input;

const READ_FAILED: u32 = u32::MAX;
const MOUSE: u32 = RIM_TYPEMOUSE.0;
const KEYBOARD: u32 = RIM_TYPEKEYBOARD.0;

pub fn record_if_user_input(handle: HRAWINPUT) {
    if read_raw_input(handle).is_some_and(|input| counts_as_user(&input)) {
        record_user_input();
    }
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
    fn the_mark_fits_the_raw_input_field() {
        assert_eq!(usize::try_from(MARK).unwrap(), INJECTION_MARK);
    }
}
