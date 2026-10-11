use deckhand_core::{Error, Result, keys::Chord};
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

use crate::{
    event::InputEvent,
    keyboard::{KeyTransition, key_event},
    keymap::{ResolvedKey, merge_modifiers, modifier_keys, not_on_layout, resolve_key},
    layout::{KeyboardLayout, SystemLayout},
    send::send,
};

pub fn hold_chord(chord: &Chord) -> Result<()> {
    let layout = SystemLayout::of_foreground_window();
    send(&held_chord_events(chord, KeyTransition::Press, &layout)?)
}

pub fn release_chord(chord: &Chord) -> Result<()> {
    let layout = SystemLayout::of_foreground_window();
    send(&held_chord_events(chord, KeyTransition::Release, &layout)?)
}

pub fn held_chord_events(
    chord: &Chord,
    transition: KeyTransition,
    layout: &impl KeyboardLayout,
) -> Result<Vec<InputEvent>> {
    let mut keys = chord_keys(chord, layout)?;
    if transition == KeyTransition::Release {
        keys.reverse();
    }
    Ok(keys.into_iter().map(|vk| key_event(vk, transition, layout)).collect())
}

fn chord_keys(chord: &Chord, layout: &impl KeyboardLayout) -> Result<Vec<VIRTUAL_KEY>> {
    let mut modifiers = chord.modifiers;
    let mut main = None;
    if let Some(key) = chord.key {
        match resolve_key(key, layout)? {
            ResolvedKey::Virtual(found) => {
                modifiers = merge_modifiers(modifiers, found.modifiers);
                main = Some(found.vk);
            }
            ResolvedKey::Unicode(c) => return Err(not_on_layout(c)),
        }
    }
    let mut keys = modifier_keys(modifiers);
    keys.extend(main);
    if keys.is_empty() {
        return Err(Error::invalid("the key chord is empty"));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{
        ErrorCode,
        keys::{Key, Modifiers, parse_chord},
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_KEYUP, VK_CONTROL, VK_SHIFT};

    use super::*;
    use crate::{event::KeyStroke, test_layout::FakeLayout};

    fn strokes(chord: &str, transition: KeyTransition) -> Vec<(VIRTUAL_KEY, bool)> {
        let events = held_chord_events(&parse_chord(chord).unwrap(), transition, &FakeLayout).unwrap();
        events
            .into_iter()
            .map(|event| match event {
                InputEvent::Key(KeyStroke { vk, flags, .. }) => (vk, flags.contains(KEYEVENTF_KEYUP)),
                InputEvent::Mouse(action) => panic!("unexpected mouse event {action:?}"),
            })
            .collect()
    }

    #[test]
    fn holding_presses_modifiers_first_and_never_releases() {
        let a = VIRTUAL_KEY(0x41);
        assert_eq!(
            strokes("ctrl+shift+a", KeyTransition::Press),
            vec![(VK_CONTROL, false), (VK_SHIFT, false), (a, false)]
        );
        assert_eq!(strokes("shift", KeyTransition::Press), vec![(VK_SHIFT, false)]);
    }

    #[test]
    fn releasing_lets_go_in_reverse() {
        let a = VIRTUAL_KEY(0x41);
        assert_eq!(
            strokes("ctrl+shift+a", KeyTransition::Release),
            vec![(a, true), (VK_SHIFT, true), (VK_CONTROL, true)]
        );
    }

    #[test]
    fn a_character_that_needs_shift_holds_shift_too() {
        let capital = Chord { modifiers: Modifiers::NONE, key: Some(Key::Char('A')) };
        let events = held_chord_events(&capital, KeyTransition::Press, &FakeLayout).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], InputEvent::Key(KeyStroke { vk: VK_SHIFT, .. })));
    }

    #[test]
    fn keys_off_the_layout_and_empty_chords_cannot_be_held() {
        let missing = Chord { modifiers: Modifiers::NONE, key: Some(Key::Char('€')) };
        let error = held_chord_events(&missing, KeyTransition::Press, &FakeLayout).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        let empty = Chord { modifiers: Modifiers::NONE, key: None };
        assert!(held_chord_events(&empty, KeyTransition::Press, &FakeLayout).is_err());
    }
}
