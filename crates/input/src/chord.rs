use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
use winpilot_core::{
    Error, Result,
    keys::{Chord, Key, Modifiers},
};

use crate::{
    batch::{Batch, with_modifiers_held},
    keyboard::{key_tap, push_unicode_tap},
    keymap::{ResolvedKey, is_dead_key_char, merge_modifiers, not_on_layout, resolve_key},
    layout::{KeyboardLayout, SystemLayout},
    send::send_batch,
};

pub fn press_chord(chord: &Chord) -> Result<()> {
    let layout = SystemLayout::of_foreground_window();
    send_batch(&chord_batch(chord, &layout)?)
}

pub(crate) fn chord_batch(chord: &Chord, layout: &impl KeyboardLayout) -> Result<Batch> {
    let Some(key) = chord.key else {
        return modifier_only_batch(chord.modifiers, layout);
    };
    if let Some(c) = lone_dead_key_char(chord, layout) {
        return Ok(unicode_batch(c));
    }
    match resolve_key(key, layout)? {
        ResolvedKey::Virtual(found) => {
            Ok(held_key_batch(merge_modifiers(chord.modifiers, found.modifiers), found.vk, layout))
        }
        ResolvedKey::Unicode(c) if chord.modifiers.is_empty() => Ok(unicode_batch(c)),
        ResolvedKey::Unicode(c) => Err(not_on_layout(c)),
    }
}

fn lone_dead_key_char(chord: &Chord, layout: &impl KeyboardLayout) -> Option<char> {
    let Some(Key::Char(c)) = chord.key else {
        return None;
    };
    (chord.modifiers.is_empty() && is_dead_key_char(c, layout)).then_some(c)
}

fn modifier_only_batch(modifiers: Modifiers, layout: &impl KeyboardLayout) -> Result<Batch> {
    if modifiers.is_empty() {
        return Err(Error::invalid("the key chord is empty"));
    }
    Ok(with_modifiers_held(modifiers, Vec::new(), layout))
}

fn held_key_batch(modifiers: Modifiers, vk: VIRTUAL_KEY, layout: &impl KeyboardLayout) -> Batch {
    with_modifiers_held(modifiers, key_tap(vk, layout).to_vec(), layout)
}

fn unicode_batch(c: char) -> Batch {
    let mut events = Vec::with_capacity(4);
    push_unicode_tap(&mut events, c);
    Batch::plain(events)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_CONTROL, VK_F5, VK_LEFT,
        VK_LWIN, VK_MENU, VK_SHIFT,
    };
    use winpilot_core::{ErrorCode, keys::NamedKey};

    use super::*;
    use crate::{
        event::{InputEvent, KeyStroke},
        test_layout::{DEAD_CIRCUMFLEX, FakeLayout},
    };

    const DOWN: KEYBD_EVENT_FLAGS = KEYBD_EVENT_FLAGS(0);

    fn strokes(events: &[InputEvent]) -> Vec<(VIRTUAL_KEY, KEYBD_EVENT_FLAGS)> {
        events
            .iter()
            .map(|event| match event {
                InputEvent::Key(KeyStroke { vk, flags, .. }) => (*vk, *flags),
                InputEvent::Mouse(action) => panic!("unexpected mouse event {action:?}"),
            })
            .collect()
    }

    fn chord(modifiers: Modifiers, key: Option<Key>) -> Chord {
        Chord { modifiers, key }
    }

    #[test]
    fn ctrl_shift_left_wraps_an_extended_key() {
        let modifiers = Modifiers { ctrl: true, shift: true, ..Modifiers::NONE };
        let batch = chord_batch(&chord(modifiers, Some(Key::Named(NamedKey::Left))), &FakeLayout).unwrap();
        assert_eq!(
            strokes(&batch.events),
            vec![
                (VK_CONTROL, DOWN),
                (VK_SHIFT, DOWN),
                (VK_LEFT, KEYEVENTF_EXTENDEDKEY),
                (VK_LEFT, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
                (VK_SHIFT, KEYEVENTF_KEYUP),
                (VK_CONTROL, KEYEVENTF_KEYUP),
            ]
        );
        assert_eq!(
            strokes(&batch.release_on_failure),
            vec![(VK_SHIFT, KEYEVENTF_KEYUP), (VK_CONTROL, KEYEVENTF_KEYUP)]
        );
    }

    #[test]
    fn all_modifiers_press_in_order_and_release_in_reverse() {
        let all = Modifiers { ctrl: true, alt: true, shift: true, win: true };
        let batch = chord_batch(&chord(all, Some(Key::Function(5))), &FakeLayout).unwrap();
        let order: Vec<VIRTUAL_KEY> = strokes(&batch.events).into_iter().map(|(vk, _)| vk).collect();
        assert_eq!(
            order,
            vec![VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_F5, VK_F5, VK_LWIN, VK_SHIFT, VK_MENU, VK_CONTROL]
        );
    }

    #[test]
    fn win_alone_presses_and_releases_win() {
        let win = Modifiers { win: true, ..Modifiers::NONE };
        let batch = chord_batch(&chord(win, None), &FakeLayout).unwrap();
        assert_eq!(
            strokes(&batch.events),
            vec![(VK_LWIN, KEYEVENTF_EXTENDEDKEY), (VK_LWIN, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP)]
        );
    }

    #[test]
    fn empty_chord_is_rejected() {
        let error = chord_batch(&chord(Modifiers::NONE, None), &FakeLayout).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn character_shift_state_is_merged_into_the_chord() {
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let batch = chord_batch(&chord(ctrl, Some(Key::Char('A'))), &FakeLayout).unwrap();
        let a = VIRTUAL_KEY(0x41);
        assert_eq!(
            strokes(&batch.events),
            vec![
                (VK_CONTROL, DOWN),
                (VK_SHIFT, DOWN),
                (a, DOWN),
                (a, KEYEVENTF_KEYUP),
                (VK_SHIFT, KEYEVENTF_KEYUP),
                (VK_CONTROL, KEYEVENTF_KEYUP),
            ]
        );
    }

    #[test]
    fn altgr_character_adds_ctrl_and_alt() {
        let batch = chord_batch(&chord(Modifiers::NONE, Some(Key::Char('@'))), &FakeLayout).unwrap();
        let order: Vec<VIRTUAL_KEY> = strokes(&batch.events).into_iter().map(|(vk, _)| vk).collect();
        let q = VIRTUAL_KEY(0x51);
        assert_eq!(order, vec![VK_CONTROL, VK_MENU, q, q, VK_MENU, VK_CONTROL]);
    }

    #[test]
    fn character_off_the_layout_falls_back_to_unicode() {
        let batch = chord_batch(&chord(Modifiers::NONE, Some(Key::Char('€'))), &FakeLayout).unwrap();
        let expected = vec![
            InputEvent::Key(KeyStroke { vk: VIRTUAL_KEY(0), scan: 0x20AC, flags: KEYEVENTF_UNICODE }),
            InputEvent::Key(KeyStroke { vk: VIRTUAL_KEY(0), scan: 0x20AC, flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP }),
        ];
        assert_eq!(batch.events, expected);
        assert!(batch.release_on_failure.is_empty());
    }

    #[test]
    fn lone_dead_key_character_is_typed_as_unicode() {
        let batch = chord_batch(&chord(Modifiers::NONE, Some(Key::Char('^'))), &FakeLayout).unwrap();
        let expected = vec![
            InputEvent::Key(KeyStroke { vk: VIRTUAL_KEY(0), scan: 0x5E, flags: KEYEVENTF_UNICODE }),
            InputEvent::Key(KeyStroke { vk: VIRTUAL_KEY(0), scan: 0x5E, flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP }),
        ];
        assert_eq!(batch.events, expected);
    }

    #[test]
    fn dead_key_with_modifiers_stays_on_its_virtual_key() {
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let batch = chord_batch(&chord(ctrl, Some(Key::Char('^'))), &FakeLayout).unwrap();
        let order: Vec<VIRTUAL_KEY> = strokes(&batch.events).into_iter().map(|(vk, _)| vk).collect();
        assert_eq!(order, vec![VK_CONTROL, DEAD_CIRCUMFLEX, DEAD_CIRCUMFLEX, VK_CONTROL]);
    }

    #[test]
    fn character_off_the_layout_with_modifiers_is_rejected() {
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let error = chord_batch(&chord(ctrl, Some(Key::Char('€'))), &FakeLayout).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(error.message, "key '€' is not on the current keyboard layout");
    }

    #[test]
    fn invalid_function_key_is_rejected() {
        let error = chord_batch(&chord(Modifiers::NONE, Some(Key::Function(30))), &FakeLayout).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
}
