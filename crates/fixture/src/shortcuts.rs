use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_F5, VK_K};
use windows::Win32::UI::WindowsAndMessaging::{
    ACCEL, CreateAcceleratorTableW, FCONTROL, FVIRTKEY, HACCEL, MSG, TranslateAcceleratorW,
};
use windows::core::{Owned, Result};

use crate::ids::{CTRL_K_COMMAND, F5_COMMAND};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    F5,
    CtrlK,
}

impl Shortcut {
    pub fn from_command(command: u16) -> Option<Self> {
        match command {
            F5_COMMAND => Some(Self::F5),
            CTRL_K_COMMAND => Some(Self::CtrlK),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::F5 => "F5",
            Self::CtrlK => "Ctrl+K",
        }
    }
}

pub struct Shortcuts(Owned<HACCEL>);

impl Shortcuts {
    pub fn create() -> Result<Self> {
        let table = [
            ACCEL { fVirt: FVIRTKEY, key: VK_F5.0, cmd: F5_COMMAND },
            ACCEL { fVirt: FVIRTKEY | FCONTROL, key: VK_K.0, cmd: CTRL_K_COMMAND },
        ];
        let handle = unsafe { CreateAcceleratorTableW(&table)? };
        Ok(Self(unsafe { Owned::new(handle) }))
    }

    pub fn translate(&self, window: HWND, message: &MSG) -> bool {
        unsafe { TranslateAcceleratorW(window, *self.0, message) != 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_map_to_shortcuts() {
        assert_eq!(Shortcut::from_command(F5_COMMAND), Some(Shortcut::F5));
        assert_eq!(Shortcut::from_command(CTRL_K_COMMAND), Some(Shortcut::CtrlK));
        assert_eq!(Shortcut::from_command(1), None);
        assert_eq!(Shortcut::from_command(2), None);
    }
}
