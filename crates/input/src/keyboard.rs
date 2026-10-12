use deckhand_core::keys::Modifiers;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

use crate::{
    event::{InputEvent, KeyStroke},
    keymap::{is_extended_key, modifier_keys},
    layout::KeyboardLayout,
};

const EXTENDED_PREFIX: u32 = 0xE0;
const PAUSE_PREFIX: u32 = 0xE1;
const NO_VIRTUAL_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyTransition {
    Press,
    Release,
}

pub(crate) fn key_event(vk: VIRTUAL_KEY, transition: KeyTransition, layout: &impl KeyboardLayout) -> InputEvent {
    let (scan, extended) = scan_and_extended(vk, layout);
    let mut flags = release_flag(transition);
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    InputEvent::Key(KeyStroke { vk, scan, flags })
}

pub(crate) fn scan_and_extended(vk: VIRTUAL_KEY, layout: &impl KeyboardLayout) -> (u16, bool) {
    let code = layout.scan_code(vk);
    (scan_byte(code), is_extended_key(vk) || code >> 8 == EXTENDED_PREFIX)
}

pub(crate) fn key_tap(vk: VIRTUAL_KEY, layout: &impl KeyboardLayout) -> [InputEvent; 2] {
    [key_event(vk, KeyTransition::Press, layout), key_event(vk, KeyTransition::Release, layout)]
}

pub(crate) fn push_unicode_tap(events: &mut Vec<InputEvent>, c: char) {
    let mut units = [0u16; 2];
    for &unit in c.encode_utf16(&mut units).iter() {
        events.push(unicode_event(unit, KeyTransition::Press));
        events.push(unicode_event(unit, KeyTransition::Release));
    }
}

pub(crate) fn modifier_presses(modifiers: Modifiers, layout: &impl KeyboardLayout) -> Vec<InputEvent> {
    modifier_keys(modifiers).into_iter().map(|vk| key_event(vk, KeyTransition::Press, layout)).collect()
}

pub(crate) fn modifier_releases(modifiers: Modifiers, layout: &impl KeyboardLayout) -> Vec<InputEvent> {
    modifier_keys(modifiers).into_iter().rev().map(|vk| key_event(vk, KeyTransition::Release, layout)).collect()
}

fn unicode_event(unit: u16, transition: KeyTransition) -> InputEvent {
    let flags = KEYEVENTF_UNICODE | release_flag(transition);
    InputEvent::Key(KeyStroke { vk: NO_VIRTUAL_KEY, scan: unit, flags })
}

fn release_flag(transition: KeyTransition) -> KEYBD_EVENT_FLAGS {
    match transition {
        KeyTransition::Press => KEYBD_EVENT_FLAGS::default(),
        KeyTransition::Release => KEYEVENTF_KEYUP,
    }
}

fn scan_byte(code: u32) -> u16 {
    if code >> 8 == PAUSE_PREFIX { 0 } else { (code & 0xFF) as u16 }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_DIVIDE, VK_LEFT, VK_PAUSE, VK_RETURN};

    use super::*;
    use crate::test_layout::FakeLayout;

    fn stroke(event: InputEvent) -> KeyStroke {
        match event {
            InputEvent::Key(stroke) => stroke,
            InputEvent::Mouse(action) => panic!("expected a key event, got {action:?}"),
        }
    }

    #[test]
    fn plain_key_carries_its_scan_code() {
        let down = stroke(key_event(VK_RETURN, KeyTransition::Press, &FakeLayout));
        assert_eq!(down, KeyStroke { vk: VK_RETURN, scan: 0x1C, flags: KEYBD_EVENT_FLAGS::default() });
        let up = stroke(key_event(VK_RETURN, KeyTransition::Release, &FakeLayout));
        assert_eq!(up.flags, KEYEVENTF_KEYUP);
    }

    #[test]
    fn listed_extended_key_gets_the_flag_and_loses_the_prefix() {
        let up = stroke(key_event(VK_LEFT, KeyTransition::Release, &FakeLayout));
        assert_eq!(up, KeyStroke { vk: VK_LEFT, scan: 0x4B, flags: KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP });
    }

    #[test]
    fn layout_prefix_also_marks_a_key_extended() {
        let down = stroke(key_event(VK_DIVIDE, KeyTransition::Press, &FakeLayout));
        assert_eq!(down, KeyStroke { vk: VK_DIVIDE, scan: 0x35, flags: KEYEVENTF_EXTENDEDKEY });
    }

    #[test]
    fn pause_prefix_drops_the_misleading_scan_code() {
        let down = stroke(key_event(VK_PAUSE, KeyTransition::Press, &FakeLayout));
        assert_eq!(down, KeyStroke { vk: VK_PAUSE, scan: 0, flags: KEYBD_EVENT_FLAGS::default() });
    }

    #[test]
    fn unicode_tap_of_bmp_char_is_one_down_up_pair() {
        let mut events = Vec::new();
        push_unicode_tap(&mut events, 'é');
        let strokes: Vec<KeyStroke> = events.into_iter().map(stroke).collect();
        assert_eq!(
            strokes,
            vec![
                KeyStroke { vk: NO_VIRTUAL_KEY, scan: 0xE9, flags: KEYEVENTF_UNICODE },
                KeyStroke { vk: NO_VIRTUAL_KEY, scan: 0xE9, flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP },
            ]
        );
    }

    #[test]
    fn modifier_releases_mirror_presses() {
        let modifiers = Modifiers { ctrl: true, shift: true, ..Modifiers::NONE };
        let presses: Vec<VIRTUAL_KEY> =
            modifier_presses(modifiers, &FakeLayout).into_iter().map(|e| stroke(e).vk).collect();
        let releases: Vec<VIRTUAL_KEY> =
            modifier_releases(modifiers, &FakeLayout).into_iter().map(|e| stroke(e).vk).collect();
        assert_eq!(presses[0], VK_CONTROL);
        assert_eq!(releases, presses.into_iter().rev().collect::<Vec<_>>());
    }
}
