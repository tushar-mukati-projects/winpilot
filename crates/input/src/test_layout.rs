use deckhand_core::keys::Modifiers;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_CONTROL, VK_DIVIDE, VK_LEFT, VK_LWIN, VK_MENU, VK_PAUSE, VK_RETURN, VK_SHIFT, VK_TAB,
};

use crate::layout::{KeyboardLayout, LayoutKey};

pub struct FakeLayout;

impl KeyboardLayout for FakeLayout {
    fn char_key(&self, c: char) -> Option<LayoutKey> {
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let altgr = Modifiers { ctrl: true, alt: true, ..Modifiers::NONE };
        match c {
            'a'..='z' => Some(LayoutKey { vk: VIRTUAL_KEY(c.to_ascii_uppercase() as u16), modifiers: Modifiers::NONE }),
            'A'..='Z' => Some(LayoutKey { vk: VIRTUAL_KEY(c as u16), modifiers: shift }),
            '@' => Some(LayoutKey { vk: VIRTUAL_KEY(0x51), modifiers: altgr }),
            _ => None,
        }
    }

    fn scan_code(&self, vk: VIRTUAL_KEY) -> u32 {
        match vk {
            VK_CONTROL => 0x1D,
            VK_SHIFT => 0x2A,
            VK_MENU => 0x38,
            VK_LWIN => 0xE05B,
            VK_LEFT => 0xE04B,
            VK_DIVIDE => 0xE035,
            VK_PAUSE => 0xE11D,
            VK_RETURN => 0x1C,
            VK_TAB => 0x0F,
            _ => 0x1E,
        }
    }
}
