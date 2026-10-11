use deckhand_core::keys::Modifiers;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU,
        VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    },
    WindowsAndMessaging::{RI_KEY_BREAK, RI_KEY_E0},
};

use crate::hotkey_binding::HotkeyBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalKey {
    pub(crate) make_code: u16,
    pub(crate) flags: u16,
    pub(crate) vk: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldKey {
    make_code: u16,
    extended: bool,
    vk: u16,
}

#[derive(Debug)]
pub(crate) struct ChordDetector {
    held: Vec<HeldKey>,
}

impl ChordDetector {
    pub(crate) const fn new() -> Self {
        Self { held: Vec::new() }
    }

    pub(crate) fn pressed(&mut self, key: PhysicalKey) -> Option<VIRTUAL_KEY> {
        let held = HeldKey {
            make_code: key.make_code,
            extended: u32::from(key.flags) & RI_KEY_E0 != 0,
            vk: key.vk,
        };
        let already = self.held.iter().position(|known| known.make_code == held.make_code && known.extended == held.extended);
        if u32::from(key.flags) & RI_KEY_BREAK != 0 {
            if let Some(index) = already {
                self.held.swap_remove(index);
            }
            return None;
        }
        if already.is_some() {
            return None;
        }
        self.held.push(held);
        Some(VIRTUAL_KEY(held.vk))
    }

    pub(crate) fn modifiers(&self) -> Modifiers {
        let held = |side_keys: &[VIRTUAL_KEY]| self.held.iter().any(|key| side_keys.contains(&VIRTUAL_KEY(key.vk)));
        Modifiers {
            ctrl: held(&[VK_CONTROL, VK_LCONTROL, VK_RCONTROL]),
            alt: held(&[VK_MENU, VK_LMENU, VK_RMENU]),
            shift: held(&[VK_SHIFT, VK_LSHIFT, VK_RSHIFT]),
            win: held(&[VK_LWIN, VK_RWIN]),
        }
    }

    pub(crate) fn forget(&mut self) {
        self.held.clear();
    }
}

pub(crate) fn required_modifiers(binding: HotkeyBinding) -> Modifiers {
    let has = |flag: HOT_KEY_MODIFIERS| binding.modifiers.0 & flag.0 != 0;
    Modifiers { ctrl: has(MOD_CONTROL), alt: has(MOD_ALT), shift: has(MOD_SHIFT), win: has(MOD_WIN) }
}

pub(crate) const fn covers(held: Modifiers, wanted: Modifiers) -> bool {
    (held.ctrl || !wanted.ctrl) && (held.alt || !wanted.alt) && (held.shift || !wanted.shift) && (held.win || !wanted.win)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, VK_ESCAPE};

    use super::*;

    const CTRL: PhysicalKey = PhysicalKey { make_code: 0x1D, flags: 0, vk: VK_CONTROL.0 };
    const RIGHT_CTRL: PhysicalKey = PhysicalKey { make_code: 0x1D, flags: RI_KEY_E0 as u16, vk: VK_CONTROL.0 };
    const ALT: PhysicalKey = PhysicalKey { make_code: 0x38, flags: 0, vk: VK_MENU.0 };
    const ESC: PhysicalKey = PhysicalKey { make_code: 0x01, flags: 0, vk: VK_ESCAPE.0 };

    fn released(key: PhysicalKey) -> PhysicalKey {
        PhysicalKey { flags: key.flags | RI_KEY_BREAK as u16, ..key }
    }

    #[test]
    fn a_key_counts_once_until_it_is_released() {
        let mut detector = ChordDetector::new();
        assert_eq!(detector.pressed(ESC), Some(VK_ESCAPE));
        assert_eq!(detector.pressed(ESC), None);
        assert_eq!(detector.pressed(released(ESC)), None);
        assert_eq!(detector.pressed(ESC), Some(VK_ESCAPE));
    }

    #[test]
    fn modifiers_follow_both_sides() {
        let mut detector = ChordDetector::new();
        detector.pressed(CTRL);
        detector.pressed(RIGHT_CTRL);
        detector.pressed(released(CTRL));
        assert!(detector.modifiers().ctrl);
        detector.pressed(released(RIGHT_CTRL));
        assert!(!detector.modifiers().ctrl);
        detector.pressed(ALT);
        assert_eq!(detector.modifiers(), Modifiers { alt: true, ..Modifiers::NONE });
        detector.forget();
        assert!(detector.modifiers().is_empty());
    }

    #[test]
    fn the_chord_needs_its_modifiers_and_tolerates_extra_ones() {
        let binding = HotkeyBinding { modifiers: MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, vk: VK_ESCAPE };
        let wanted = required_modifiers(binding);
        assert_eq!(wanted, Modifiers { ctrl: true, alt: true, ..Modifiers::NONE });
        assert!(covers(Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }, wanted));
        assert!(covers(Modifiers { ctrl: true, alt: true, shift: true, win: false }, wanted));
        assert!(!covers(Modifiers { ctrl: true, ..Modifiers::NONE }, wanted));
    }
}
