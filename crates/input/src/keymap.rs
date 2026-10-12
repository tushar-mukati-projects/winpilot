use deckhand_core::{
    Error, Result,
    keys::{Key, Modifiers, NamedKey},
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_APPS, VK_BACK, VK_BROWSER_BACK, VK_BROWSER_FORWARD, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN,
    VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
    VK_MEDIA_PREV_TRACK, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_PAUSE, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU,
    VK_RWIN, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};

use crate::layout::{KeyboardLayout, LayoutKey};

const FUNCTION_KEYS: std::ops::RangeInclusive<u8> = 1..=24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolvedKey {
    Virtual(LayoutKey),
    Unicode(char),
}

pub(crate) fn resolve_key(key: Key, layout: &impl KeyboardLayout) -> Result<ResolvedKey> {
    match key {
        Key::Named(named) => Ok(unmodified(named_key_vk(named))),
        Key::Function(number) => function_key_vk(number).map(unmodified),
        Key::Char(c) => Ok(layout.char_key(c).map_or(ResolvedKey::Unicode(c), ResolvedKey::Virtual)),
    }
}

pub(crate) fn is_dead_key_char(c: char, layout: &impl KeyboardLayout) -> bool {
    layout.char_key(c).is_some_and(|found| layout.is_dead_key(found.vk))
}

pub(crate) fn not_on_layout(c: char) -> Error {
    Error::invalid(format!("key '{c}' is not on the current keyboard layout"))
}

pub(crate) fn merge_modifiers(first: Modifiers, second: Modifiers) -> Modifiers {
    Modifiers {
        ctrl: first.ctrl || second.ctrl,
        alt: first.alt || second.alt,
        shift: first.shift || second.shift,
        win: first.win || second.win,
    }
}

pub(crate) fn modifier_keys(modifiers: Modifiers) -> Vec<VIRTUAL_KEY> {
    [(modifiers.ctrl, VK_CONTROL), (modifiers.alt, VK_MENU), (modifiers.shift, VK_SHIFT), (modifiers.win, VK_LWIN)]
        .into_iter()
        .filter_map(|(held, vk)| held.then_some(vk))
        .collect()
}

pub(crate) fn is_extended_key(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_INSERT
            | VK_DELETE
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_UP
            | VK_DOWN
            | VK_LEFT
            | VK_RIGHT
            | VK_NUMLOCK
            | VK_SNAPSHOT
            | VK_APPS
            | VK_LWIN
            | VK_RWIN
            | VK_RCONTROL
            | VK_RMENU
            | VK_VOLUME_UP
            | VK_VOLUME_DOWN
            | VK_VOLUME_MUTE
            | VK_MEDIA_PLAY_PAUSE
            | VK_MEDIA_NEXT_TRACK
            | VK_MEDIA_PREV_TRACK
            | VK_BROWSER_BACK
            | VK_BROWSER_FORWARD
    )
}

fn unmodified(vk: VIRTUAL_KEY) -> ResolvedKey {
    ResolvedKey::Virtual(LayoutKey { vk, modifiers: Modifiers::NONE })
}

fn function_key_vk(number: u8) -> Result<VIRTUAL_KEY> {
    if !FUNCTION_KEYS.contains(&number) {
        return Err(Error::invalid(format!("F{number} is not a function key; use F1 to F24")));
    }
    Ok(VIRTUAL_KEY(VK_F1.0 + u16::from(number) - 1))
}

fn named_key_vk(key: NamedKey) -> VIRTUAL_KEY {
    match key {
        NamedKey::Enter => VK_RETURN,
        NamedKey::Escape => VK_ESCAPE,
        NamedKey::Tab => VK_TAB,
        NamedKey::Space => VK_SPACE,
        NamedKey::Backspace => VK_BACK,
        NamedKey::Delete => VK_DELETE,
        NamedKey::Insert => VK_INSERT,
        NamedKey::Home => VK_HOME,
        NamedKey::End => VK_END,
        NamedKey::PageUp => VK_PRIOR,
        NamedKey::PageDown => VK_NEXT,
        NamedKey::Up => VK_UP,
        NamedKey::Down => VK_DOWN,
        NamedKey::Left => VK_LEFT,
        NamedKey::Right => VK_RIGHT,
        NamedKey::PrintScreen => VK_SNAPSHOT,
        NamedKey::Pause => VK_PAUSE,
        NamedKey::CapsLock => VK_CAPITAL,
        NamedKey::NumLock => VK_NUMLOCK,
        NamedKey::ScrollLock => VK_SCROLL,
        NamedKey::ContextMenu => VK_APPS,
        NamedKey::VolumeUp => VK_VOLUME_UP,
        NamedKey::VolumeDown => VK_VOLUME_DOWN,
        NamedKey::VolumeMute => VK_VOLUME_MUTE,
        NamedKey::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        NamedKey::MediaNext => VK_MEDIA_NEXT_TRACK,
        NamedKey::MediaPrevious => VK_MEDIA_PREV_TRACK,
        NamedKey::BrowserBack => VK_BROWSER_BACK,
        NamedKey::BrowserForward => VK_BROWSER_FORWARD,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_F5, VK_F12, VK_F24};

    use super::*;
    use crate::test_layout::FakeLayout;

    fn vk_of(key: Key) -> VIRTUAL_KEY {
        match resolve_key(key, &FakeLayout).unwrap() {
            ResolvedKey::Virtual(found) => found.vk,
            ResolvedKey::Unicode(c) => panic!("expected a virtual key, got unicode {c}"),
        }
    }

    #[test]
    fn named_keys_map_to_their_virtual_keys() {
        assert_eq!(vk_of(Key::Named(NamedKey::Enter)), VK_RETURN);
        assert_eq!(vk_of(Key::Named(NamedKey::Backspace)), VK_BACK);
        assert_eq!(vk_of(Key::Named(NamedKey::PageUp)), VK_PRIOR);
        assert_eq!(vk_of(Key::Named(NamedKey::PageDown)), VK_NEXT);
        assert_eq!(vk_of(Key::Named(NamedKey::PrintScreen)), VK_SNAPSHOT);
        assert_eq!(vk_of(Key::Named(NamedKey::CapsLock)), VK_CAPITAL);
        assert_eq!(vk_of(Key::Named(NamedKey::ContextMenu)), VK_APPS);
        assert_eq!(vk_of(Key::Named(NamedKey::MediaPrevious)), VK_MEDIA_PREV_TRACK);
    }

    #[test]
    fn function_keys_cover_f1_to_f24() {
        assert_eq!(vk_of(Key::Function(1)), VK_F1);
        assert_eq!(vk_of(Key::Function(5)), VK_F5);
        assert_eq!(vk_of(Key::Function(12)), VK_F12);
        assert_eq!(vk_of(Key::Function(24)), VK_F24);
    }

    #[test]
    fn function_keys_outside_the_range_are_rejected() {
        for number in [0, 25, 255] {
            let error = resolve_key(Key::Function(number), &FakeLayout).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument);
        }
    }

    #[test]
    fn characters_resolve_through_the_layout() {
        let resolved = resolve_key(Key::Char('A'), &FakeLayout).unwrap();
        let expected = LayoutKey { vk: VIRTUAL_KEY(0x41), modifiers: Modifiers { shift: true, ..Modifiers::NONE } };
        assert_eq!(resolved, ResolvedKey::Virtual(expected));
        assert_eq!(resolve_key(Key::Char('€'), &FakeLayout).unwrap(), ResolvedKey::Unicode('€'));
    }

    #[test]
    fn dead_key_characters_are_recognised() {
        assert!(is_dead_key_char('^', &FakeLayout));
        assert!(!is_dead_key_char('a', &FakeLayout));
        assert!(!is_dead_key_char('€', &FakeLayout));
    }

    #[test]
    fn modifier_keys_are_ordered_ctrl_alt_shift_win() {
        let all = Modifiers { ctrl: true, alt: true, shift: true, win: true };
        assert_eq!(modifier_keys(all), vec![VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN]);
        assert_eq!(modifier_keys(Modifiers { shift: true, win: true, ..Modifiers::NONE }), vec![VK_SHIFT, VK_LWIN]);
        assert!(modifier_keys(Modifiers::NONE).is_empty());
    }

    #[test]
    fn merging_modifiers_is_a_union() {
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        assert_eq!(merge_modifiers(ctrl, shift), Modifiers { ctrl: true, shift: true, ..Modifiers::NONE });
        assert_eq!(merge_modifiers(ctrl, ctrl), ctrl);
    }

    #[test]
    fn extended_keys_match_the_list() {
        for vk in [VK_INSERT, VK_DELETE, VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_UP, VK_LEFT, VK_NUMLOCK, VK_SNAPSHOT] {
            assert!(is_extended_key(vk), "{vk:?}");
        }
        for vk in [VK_APPS, VK_RCONTROL, VK_RMENU, VK_VOLUME_UP, VK_MEDIA_PLAY_PAUSE, VK_BROWSER_BACK, VK_LWIN] {
            assert!(is_extended_key(vk), "{vk:?}");
        }
        for vk in [VK_RETURN, VK_TAB, VK_SPACE, VK_BACK, VK_ESCAPE, VK_CONTROL, VK_SHIFT, VK_MENU, VK_F1, VK_PAUSE] {
            assert!(!is_extended_key(vk), "{vk:?}");
        }
    }
}
