use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VIRTUAL_KEY,
};
use winpilot_core::{
    Error, Result,
    keys::{Chord, Modifiers},
};

use crate::{
    keymap::{ResolvedKey, merge_modifiers, not_on_layout, resolve_key},
    layout::KeyboardLayout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HotkeyBinding {
    pub(crate) modifiers: HOT_KEY_MODIFIERS,
    pub(crate) vk: VIRTUAL_KEY,
}

pub(crate) fn hotkey_binding(chord: &Chord, layout: &impl KeyboardLayout) -> Result<HotkeyBinding> {
    let Some(key) = chord.key else {
        return Err(Error::invalid("a hotkey needs a key, not just modifiers"));
    };
    match resolve_key(key, layout)? {
        ResolvedKey::Virtual(found) => {
            let modifiers = hotkey_modifiers(merge_modifiers(chord.modifiers, found.modifiers));
            Ok(HotkeyBinding { modifiers, vk: found.vk })
        }
        ResolvedKey::Unicode(c) => Err(not_on_layout(c)),
    }
}

fn hotkey_modifiers(modifiers: Modifiers) -> HOT_KEY_MODIFIERS {
    [(modifiers.ctrl, MOD_CONTROL), (modifiers.alt, MOD_ALT), (modifiers.shift, MOD_SHIFT), (modifiers.win, MOD_WIN)]
        .into_iter()
        .filter(|(held, _)| *held)
        .fold(MOD_NOREPEAT, |all, (_, flag)| all | flag)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_F9};
    use winpilot_core::{
        ErrorCode,
        keys::{Key, NamedKey},
    };

    use super::*;
    use crate::test_layout::FakeLayout;

    fn binding(modifiers: Modifiers, key: Option<Key>) -> Result<HotkeyBinding> {
        hotkey_binding(&Chord { modifiers, key }, &FakeLayout)
    }

    #[test]
    fn default_kill_switch_maps_to_ctrl_alt_escape() {
        let modifiers = Modifiers { ctrl: true, alt: true, ..Modifiers::NONE };
        let found = binding(modifiers, Some(Key::Named(NamedKey::Escape))).unwrap();
        assert_eq!(found.vk, VK_ESCAPE);
        assert_eq!(found.modifiers, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT);
    }

    #[test]
    fn win_function_key_maps_with_norepeat() {
        let found = binding(Modifiers { win: true, ..Modifiers::NONE }, Some(Key::Function(9))).unwrap();
        assert_eq!(found, HotkeyBinding { modifiers: MOD_WIN | MOD_NOREPEAT, vk: VK_F9 });
    }

    #[test]
    fn character_shift_state_is_merged() {
        let found = binding(Modifiers { ctrl: true, ..Modifiers::NONE }, Some(Key::Char('A'))).unwrap();
        assert_eq!(found, HotkeyBinding { modifiers: MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, vk: VIRTUAL_KEY(0x41) });
    }

    #[test]
    fn dead_key_characters_stay_on_their_virtual_key() {
        let found = binding(Modifiers { ctrl: true, ..Modifiers::NONE }, Some(Key::Char('^'))).unwrap();
        assert_eq!(found, HotkeyBinding { modifiers: MOD_CONTROL | MOD_NOREPEAT, vk: VIRTUAL_KEY(0xDD) });
    }

    #[test]
    fn modifier_only_chord_is_rejected() {
        let error = binding(Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn unmappable_keys_are_rejected() {
        let off_layout = binding(Modifiers { ctrl: true, ..Modifiers::NONE }, Some(Key::Char('€'))).unwrap_err();
        assert_eq!(off_layout.code, ErrorCode::InvalidArgument);
        let bad_function = binding(Modifiers::NONE, Some(Key::Function(0))).unwrap_err();
        assert_eq!(bad_function.code, ErrorCode::InvalidArgument);
    }
}
