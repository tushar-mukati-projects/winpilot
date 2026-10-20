use windows::Win32::UI::{
    Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_BACK, VK_ESCAPE, VK_RETURN, VK_TAB},
    WindowsAndMessaging::{WM_CHAR, WM_KEYDOWN, WM_KEYUP},
};
use winpilot_core::{
    Error, ErrorCode, Result,
    keys::{Chord, Key, NamedKey},
};

use crate::{
    keyboard::scan_and_extended,
    keymap::{ResolvedKey, resolve_key},
    layout::KeyboardLayout,
    message_packing::{KeyCode, char_lparam, key_lparam},
    message_plan::{Sent, message},
    text::normalize_newlines,
};

const CARRIAGE_RETURN: u16 = 0x0D;
const BACKSPACE: u16 = 0x08;
const TAB: u16 = 0x09;
const ESCAPE: u16 = 0x1B;
const QT_CLASS_PREFIX: &str = "Qt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toolkit {
    Qt,
    Other,
}

impl Toolkit {
    pub(crate) fn of_class(class: &str) -> Self {
        if class.starts_with(QT_CLASS_PREFIX) { Self::Qt } else { Self::Other }
    }
}

pub(crate) fn text_plan(text: &str, enter: KeyCode, toolkit: Toolkit) -> Vec<Sent> {
    let mut plan = Vec::with_capacity(text.len());
    for c in normalize_newlines(text).chars() {
        if c == '\n' {
            plan.extend(typing_press(enter, CARRIAGE_RETURN, toolkit));
        } else {
            push_char(&mut plan, c);
        }
    }
    plan
}

pub(crate) fn keys_plan(chords: &[Chord], layout: &impl KeyboardLayout, toolkit: Toolkit) -> Result<Vec<Sent>> {
    let mut plan = Vec::new();
    for chord in chords {
        plan.extend(chord_plan(chord, layout, toolkit)?);
    }
    Ok(plan)
}

pub(crate) fn key_code(vk: VIRTUAL_KEY, layout: &impl KeyboardLayout) -> KeyCode {
    let (scan, extended) = scan_and_extended(vk, layout);
    KeyCode { vk: vk.0, scan, extended }
}

fn chord_plan(chord: &Chord, layout: &impl KeyboardLayout, toolkit: Toolkit) -> Result<Vec<Sent>> {
    if !chord.modifiers.is_empty() {
        return Err(modifiers_need_the_keyboard());
    }
    let Some(key) = chord.key else { return Err(Error::invalid("the key chord is empty")) };
    if let Some((vk, typed)) = control_character(key) {
        return Ok(typing_press(key_code(vk, layout), typed, toolkit));
    }
    match key {
        Key::Char(c) => Ok(char_plan(c)),
        Key::Named(_) | Key::Function(_) => match resolve_key(key, layout)? {
            ResolvedKey::Virtual(found) => Ok(press(key_code(found.vk, layout)).to_vec()),
            ResolvedKey::Unicode(c) => Ok(char_plan(c)),
        },
    }
}

fn control_character(key: Key) -> Option<(VIRTUAL_KEY, u16)> {
    match key {
        Key::Named(NamedKey::Enter) => Some((VK_RETURN, CARRIAGE_RETURN)),
        Key::Named(NamedKey::Backspace) => Some((VK_BACK, BACKSPACE)),
        Key::Named(NamedKey::Tab) => Some((VK_TAB, TAB)),
        Key::Named(NamedKey::Escape) => Some((VK_ESCAPE, ESCAPE)),
        _ => None,
    }
}

fn modifiers_need_the_keyboard() -> Error {
    Error::new(
        ErrorCode::NeedsUser,
        "key chords with modifiers need the real keyboard: apps read ctrl, alt, shift and win from the keyboard state, which window messages can't set (drop background)",
    )
}

fn char_plan(c: char) -> Vec<Sent> {
    let mut plan = Vec::with_capacity(2);
    push_char(&mut plan, c);
    plan
}

fn push_char(plan: &mut Vec<Sent>, c: char) {
    let mut units = [0u16; 2];
    for unit in c.encode_utf16(&mut units).iter() {
        plan.push(message(WM_CHAR, usize::from(*unit), char_lparam()));
    }
}

fn press(code: KeyCode) -> [Sent; 2] {
    [
        message(WM_KEYDOWN, usize::from(code.vk), key_lparam(code, false)),
        message(WM_KEYUP, usize::from(code.vk), key_lparam(code, true)),
    ]
}

fn typing_press(code: KeyCode, typed: u16, toolkit: Toolkit) -> Vec<Sent> {
    let [down, up] = press(code);
    match toolkit {
        Toolkit::Qt => vec![down, up],
        Toolkit::Other => vec![down, message(WM_CHAR, usize::from(typed), key_lparam(code, false)), up],
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_END, VK_LEFT};
    use winpilot_core::keys::{Modifiers, parse_keys};

    use super::*;
    use crate::{
        message_plan::{ids, messages},
        test_layout::FakeLayout,
    };

    fn keys(spec: &str) -> Result<Vec<Sent>> {
        keys_for(spec, Toolkit::Other)
    }

    fn keys_for(spec: &str, toolkit: Toolkit) -> Result<Vec<Sent>> {
        keys_plan(&parse_keys(spec).unwrap(), &FakeLayout, toolkit)
    }

    #[test]
    fn text_is_one_char_message_per_utf16_unit_and_newlines_press_enter() {
        let enter = key_code(VK_RETURN, &FakeLayout);
        let sent = messages(&text_plan("a😀\r\nb", enter, Toolkit::Other));
        assert_eq!(
            sent,
            vec![
                (WM_CHAR, 0x61, 1),
                (WM_CHAR, 0xD83D, 1),
                (WM_CHAR, 0xDE00, 1),
                (WM_KEYDOWN, 0x0D, key_lparam(enter, false)),
                (WM_CHAR, 0x0D, key_lparam(enter, false)),
                (WM_KEYUP, 0x0D, key_lparam(enter, true)),
                (WM_CHAR, 0x62, 1),
            ]
        );
        assert!(text_plan("", enter, Toolkit::Other).is_empty());
    }

    #[test]
    fn navigation_keys_are_extended_and_backspace_is_not() {
        let end = key_code(VK_END, &FakeLayout);
        assert!(end.extended);
        assert_eq!(
            messages(&keys("end").unwrap()),
            vec![
                (WM_KEYDOWN, usize::from(VK_END.0), key_lparam(end, false)),
                (WM_KEYUP, usize::from(VK_END.0), key_lparam(end, true)),
            ]
        );
        assert!(key_code(VK_LEFT, &FakeLayout).extended);
        assert!(!key_code(VK_BACK, &FakeLayout).extended);
        assert_eq!(ids(&keys("home").unwrap()), vec![WM_KEYDOWN, WM_KEYUP]);
    }

    #[test]
    fn keys_that_type_a_control_character_send_it_between_down_and_up() {
        for (spec, typed) in [("backspace", 0x08), ("tab", 0x09), ("escape", 0x1B), ("enter", 0x0D)] {
            let sent = messages(&keys(spec).unwrap());
            assert_eq!(
                sent.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
                vec![WM_KEYDOWN, WM_CHAR, WM_KEYUP],
                "{spec}"
            );
            assert_eq!(sent[1].1, typed, "{spec}");
        }
    }

    #[test]
    fn qt_windows_get_control_keys_without_a_character_message() {
        for spec in ["backspace", "tab", "escape", "enter"] {
            assert_eq!(ids(&keys_for(spec, Toolkit::Qt).unwrap()), vec![WM_KEYDOWN, WM_KEYUP], "{spec}");
        }
        assert_eq!(ids(&keys_for("a enter", Toolkit::Qt).unwrap()), vec![WM_CHAR, WM_KEYDOWN, WM_KEYUP]);
        let enter = key_code(VK_RETURN, &FakeLayout);
        assert_eq!(ids(&text_plan("a\nb", enter, Toolkit::Qt)), vec![WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_CHAR]);
    }

    #[test]
    fn only_qt_window_classes_count_as_qt() {
        for class in ["Qt5152QWindowIcon", "Qt663QWindowIcon", "Qt6QWindowIcon"] {
            assert_eq!(Toolkit::of_class(class), Toolkit::Qt, "{class}");
        }
        for class in ["Edit", "Chrome_RenderWidgetHostHWND", ""] {
            assert_eq!(Toolkit::of_class(class), Toolkit::Other, "{class}");
        }
    }

    #[test]
    fn enter_types_a_carriage_return_between_its_down_and_up() {
        assert_eq!(ids(&keys("enter").unwrap()), vec![WM_KEYDOWN, WM_CHAR, WM_KEYUP]);
        assert_eq!(messages(&keys("enter").unwrap())[1].1, 0x0D);
    }

    #[test]
    fn character_keys_are_typed_as_characters() {
        assert_eq!(messages(&keys("a").unwrap()), vec![(WM_CHAR, 0x61, 1)]);
        assert_eq!(ids(&keys("a b enter").unwrap()), vec![WM_CHAR, WM_CHAR, WM_KEYDOWN, WM_CHAR, WM_KEYUP]);
    }

    #[test]
    fn modifier_chords_are_refused_before_anything_is_sent() {
        for spec in ["ctrl+a", "shift+tab", "end ctrl+s", "alt"] {
            let error = keys(spec).unwrap_err();
            assert_eq!(error.code, ErrorCode::NeedsUser, "{spec}");
            assert!(error.message.contains("drop background"), "{}", error.message);
        }
        let empty = Chord { modifiers: Modifiers::NONE, key: None };
        assert_eq!(keys_plan(&[empty], &FakeLayout, Toolkit::Other).unwrap_err().code, ErrorCode::InvalidArgument);
    }
}
