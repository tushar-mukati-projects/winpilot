use deckhand_core::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_RETURN, VK_TAB};

use crate::{
    event::InputEvent,
    keyboard::{key_tap, push_unicode_tap},
    layout::{KeyboardLayout, SystemLayout},
    send::send,
};

pub fn type_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let layout = SystemLayout::of_foreground_window();
    send(&text_events(text, &layout))
}

pub(crate) fn text_events(text: &str, layout: &impl KeyboardLayout) -> Vec<InputEvent> {
    let normalized = normalize_newlines(text);
    let mut events = Vec::with_capacity(normalized.len() * 2);
    for c in normalized.chars() {
        push_char(&mut events, c, layout);
    }
    events
}

pub(crate) fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn push_char(events: &mut Vec<InputEvent>, c: char, layout: &impl KeyboardLayout) {
    match c {
        '\n' => events.extend(key_tap(VK_RETURN, layout)),
        '\t' => events.extend(key_tap(VK_TAB, layout)),
        _ => push_unicode_tap(events, c),
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
    };

    use super::*;
    use crate::{event::KeyStroke, test_layout::FakeLayout};

    fn unicode(unit: u16, up: bool) -> KeyStroke {
        let flags = if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE };
        KeyStroke { vk: VIRTUAL_KEY(0), scan: unit, flags }
    }

    fn strokes(text: &str) -> Vec<KeyStroke> {
        text_events(text, &FakeLayout)
            .into_iter()
            .map(|event| match event {
                InputEvent::Key(stroke) => stroke,
                InputEvent::Mouse(action) => panic!("unexpected mouse event {action:?}"),
            })
            .collect()
    }

    fn virtual_keys(text: &str) -> Vec<VIRTUAL_KEY> {
        strokes(text).into_iter().map(|stroke| stroke.vk).collect()
    }

    #[test]
    fn crlf_and_lone_cr_become_newlines() {
        assert_eq!(normalize_newlines("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(normalize_newlines("\r\r\n"), "\n\n");
    }

    #[test]
    fn newline_and_tab_become_real_keys() {
        let zero = VIRTUAL_KEY(0);
        assert_eq!(
            virtual_keys("a\r\nb\tc"),
            vec![zero, zero, VK_RETURN, VK_RETURN, zero, zero, VK_TAB, VK_TAB, zero, zero]
        );
        let enter = strokes("\n");
        assert_eq!(enter[0], KeyStroke { vk: VK_RETURN, scan: 0x1C, flags: KEYBD_EVENT_FLAGS::default() });
        assert_eq!(enter[1].flags, KEYEVENTF_KEYUP);
    }

    #[test]
    fn crlf_produces_a_single_enter() {
        assert_eq!(virtual_keys("\r\n"), vec![VK_RETURN, VK_RETURN]);
    }

    #[test]
    fn plain_characters_are_unicode_down_up_pairs() {
        assert_eq!(
            strokes("hi"),
            vec![unicode(0x68, false), unicode(0x68, true), unicode(0x69, false), unicode(0x69, true)]
        );
    }

    #[test]
    fn emoji_becomes_a_surrogate_pair_of_taps() {
        assert_eq!(
            strokes("😀"),
            vec![unicode(0xD83D, false), unicode(0xD83D, true), unicode(0xDE00, false), unicode(0xDE00, true)]
        );
    }

    #[test]
    fn empty_text_has_no_events() {
        assert!(text_events("", &FakeLayout).is_empty());
    }

    #[test]
    fn long_text_has_two_events_per_character() {
        assert_eq!(text_events(&"x".repeat(2_000), &FakeLayout).len(), 4_000);
    }
}
