use std::mem::size_of;

use deckhand_core::{Context, Result};
use windows::Win32::{
    Foundation::HWND,
    UI::Input::{RAWINPUTDEVICE, RAWINPUTDEVICE_FLAGS, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices},
};

const GENERIC_DESKTOP_PAGE: u16 = 0x01;
const MOUSE_USAGE: u16 = 0x02;
const KEYBOARD_USAGE: u16 = 0x06;

pub struct RawInputRegistration {
    _private: (),
}

impl RawInputRegistration {
    pub fn register(target: HWND) -> Result<Self> {
        register_devices(&devices(RIDEV_INPUTSINK, target)).context("registering for raw mouse and keyboard input")?;
        Ok(Self { _private: () })
    }
}

impl Drop for RawInputRegistration {
    fn drop(&mut self) {
        let _ = register_devices(&devices(RIDEV_REMOVE, HWND::default()));
    }
}

fn register_devices(devices: &[RAWINPUTDEVICE]) -> windows::core::Result<()> {
    unsafe { RegisterRawInputDevices(devices, size_of::<RAWINPUTDEVICE>() as u32) }
}

fn devices(flags: RAWINPUTDEVICE_FLAGS, target: HWND) -> [RAWINPUTDEVICE; 2] {
    [MOUSE_USAGE, KEYBOARD_USAGE].map(|usage| RAWINPUTDEVICE {
        usUsagePage: GENERIC_DESKTOP_PAGE,
        usUsage: usage,
        dwFlags: flags,
        hwndTarget: target,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::*;

    #[test]
    fn sink_registration_covers_mouse_and_keyboard_for_one_window() {
        let window = HWND(0x40 as *mut c_void);
        let list = devices(RIDEV_INPUTSINK, window);
        assert_eq!(list.map(|device| (device.usUsagePage, device.usUsage)), [(1, 2), (1, 6)]);
        assert!(list.iter().all(|device| device.dwFlags == RIDEV_INPUTSINK && device.hwndTarget == window));
    }

    #[test]
    fn removal_names_no_window() {
        let list = devices(RIDEV_REMOVE, HWND::default());
        assert!(list.iter().all(|device| device.dwFlags == RIDEV_REMOVE && device.hwndTarget.is_invalid()));
    }
}
