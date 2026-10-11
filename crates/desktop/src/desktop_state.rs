use std::{mem::size_of_val, ptr};

use windows::Win32::{
    Foundation::HANDLE,
    System::StationsAndDesktops::{
        CloseDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetUserObjectInformationW, HDESK, OpenInputDesktop,
        UOI_NAME,
    },
};

const USER_DESKTOP: &str = "Default";
const NAME_CAPACITY: usize = 256;

struct InputDesktop(HDESK);

impl InputDesktop {
    fn open() -> Option<Self> {
        let desktop = unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) }.ok()?;
        (!desktop.is_invalid()).then_some(Self(desktop))
    }

    fn name(&self) -> Option<String> {
        let mut buffer = [0u16; NAME_CAPACITY];
        let mut needed = 0u32;
        unsafe {
            GetUserObjectInformationW(
                HANDLE(self.0.0),
                UOI_NAME,
                Some(buffer.as_mut_ptr().cast()),
                size_of_val(&buffer) as u32,
                Some(ptr::from_mut(&mut needed)),
            )
        }
        .ok()?;
        let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

impl Drop for InputDesktop {
    fn drop(&mut self) {
        let _ = unsafe { CloseDesktop(self.0) };
    }
}

pub fn secure_desktop_active() -> bool {
    let Some(desktop) = InputDesktop::open() else { return true };
    desktop.name().is_none_or(|name| !name.eq_ignore_ascii_case(USER_DESKTOP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_desktop_is_the_input_desktop() {
        assert!(!secure_desktop_active());
    }
}
