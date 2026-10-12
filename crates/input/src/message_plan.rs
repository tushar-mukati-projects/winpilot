use std::time::Duration;

use deckhand_core::{
    Error, ErrorCode, Point, Result,
    keys::{Chord, Key, NamedKey},
    protocol::{Direction, MouseButton},
};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_BACK, VK_ESCAPE, VK_RETURN, VK_TAB},
    WindowsAndMessaging::{WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_MOUSEMOVE},
};

use crate::{
    keyboard::scan_and_extended,
    keymap::{ResolvedKey, resolve_key},
    layout::KeyboardLayout,
    message_packing::{KeyCode, button_messages, char_lparam, key_lparam, point_lparam, wheel, wheel_wparam},
    mouse::MAX_SCROLL_NOTCHES,
    text::normalize_newlines,
};

const SWEEP_OFFSETS: [i32; 5] = [60, 40, 20, 8, 0];
const SWEEP_PAUSE: Duration = Duration::from_millis(80);
const PRESS_HOLD: Duration = Duration::from_millis(120);
const CLICK_GAP: Duration = Duration::from_millis(60);
const WHEEL_PAUSE: Duration = Duration::from_millis(30);
const DRAG_STEPS: u32 = 8;
const NOTHING_HELD: usize = 0;
const CARRIAGE_RETURN: u16 = 0x0D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sent {
    Message { id: u32, wparam: usize, lparam: isize },
    Pause(Duration),
}

const fn message(id: u32, wparam: usize, lparam: isize) -> Sent {
    Sent::Message { id, wparam, lparam }
}

pub(crate) fn click_plan(client: Point, button: MouseButton, count: u8) -> Vec<Sent> {
    let messages = button_messages(button);
    let at = point_lparam(client);
    let mut plan = hover_sweep(client);
    plan.extend([
        message(messages.down, messages.held, at),
        Sent::Pause(PRESS_HOLD),
        message(messages.up, NOTHING_HELD, at),
    ]);
    for follow_up in 1..count {
        let press = if follow_up == 1 { messages.double } else { messages.down };
        plan.extend([
            Sent::Pause(CLICK_GAP),
            message(press, messages.held, at),
            Sent::Pause(CLICK_GAP),
            message(messages.up, NOTHING_HELD, at),
        ]);
    }
    plan
}

pub(crate) fn release_plan(client: Point, button: MouseButton) -> Sent {
    message(button_messages(button).up, NOTHING_HELD, point_lparam(client))
}

pub(crate) fn drag_plan(from: Point, to: Point, button: MouseButton, travel: Duration) -> Vec<Sent> {
    let messages = button_messages(button);
    let mut plan = hover_sweep(from);
    plan.push(message(messages.down, messages.held, point_lparam(from)));
    let pause = travel / DRAG_STEPS;
    for step in 1..=DRAG_STEPS {
        plan.push(Sent::Pause(pause));
        plan.push(message(WM_MOUSEMOVE, messages.held, point_lparam(between(from, to, step))));
    }
    plan.extend([Sent::Pause(PRESS_HOLD), message(messages.up, NOTHING_HELD, point_lparam(to))]);
    plan
}

pub(crate) fn scroll_plan(client: Point, screen: Point, direction: Direction, notches: u32) -> Result<Vec<Sent>> {
    if notches == 0 {
        return Err(Error::invalid("scroll needs at least one notch"));
    }
    let wheel = wheel(direction);
    let mut plan = vec![message(WM_MOUSEMOVE, NOTHING_HELD, point_lparam(client))];
    for _ in 0..notches.min(MAX_SCROLL_NOTCHES) {
        plan.push(Sent::Pause(WHEEL_PAUSE));
        plan.push(message(wheel.message, wheel_wparam(wheel.delta), point_lparam(screen)));
    }
    Ok(plan)
}

pub(crate) fn text_plan(text: &str, enter: KeyCode) -> Vec<Sent> {
    let mut plan = Vec::with_capacity(text.len());
    for c in normalize_newlines(text).chars() {
        if c == '\n' {
            plan.extend(typing_press(enter, CARRIAGE_RETURN));
        } else {
            push_char(&mut plan, c);
        }
    }
    plan
}

pub(crate) fn keys_plan(chords: &[Chord], layout: &impl KeyboardLayout) -> Result<Vec<Sent>> {
    let mut plan = Vec::new();
    for chord in chords {
        plan.extend(chord_plan(chord, layout)?);
    }
    Ok(plan)
}

pub(crate) fn key_code(vk: VIRTUAL_KEY, layout: &impl KeyboardLayout) -> KeyCode {
    let (scan, extended) = scan_and_extended(vk, layout);
    KeyCode { vk: vk.0, scan, extended }
}

fn chord_plan(chord: &Chord, layout: &impl KeyboardLayout) -> Result<Vec<Sent>> {
    if !chord.modifiers.is_empty() {
        return Err(modifiers_need_the_keyboard());
    }
    let Some(key) = chord.key else { return Err(Error::invalid("the key chord is empty")) };
    if let Some((vk, typed)) = control_character(key) {
        return Ok(typing_press(key_code(vk, layout), typed).to_vec());
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
        Key::Named(NamedKey::Backspace) => Some((VK_BACK, 0x08)),
        Key::Named(NamedKey::Tab) => Some((VK_TAB, 0x09)),
        Key::Named(NamedKey::Escape) => Some((VK_ESCAPE, 0x1B)),
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

fn typing_press(code: KeyCode, typed: u16) -> [Sent; 3] {
    let [down, up] = press(code);
    [down, message(WM_CHAR, usize::from(typed), key_lparam(code, false)), up]
}

fn hover_sweep(client: Point) -> Vec<Sent> {
    let mut plan = Vec::with_capacity(SWEEP_OFFSETS.len() * 2);
    for offset in SWEEP_OFFSETS {
        let point = Point::new(client.x.saturating_sub(offset), client.y);
        plan.extend([message(WM_MOUSEMOVE, NOTHING_HELD, point_lparam(point)), Sent::Pause(SWEEP_PAUSE)]);
    }
    plan
}

fn between(from: Point, to: Point, step: u32) -> Point {
    let along = |start: i32, end: i32| {
        let offset = (i64::from(end) - i64::from(start)) * i64::from(step) / i64::from(DRAG_STEPS);
        i32::try_from(i64::from(start) + offset).unwrap_or(end)
    };
    Point::new(along(from.x, to.x), along(from.y, to.y))
}

#[cfg(test)]
mod tests {
    use deckhand_core::keys::{Modifiers, parse_keys};
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{VK_BACK, VK_END, VK_LEFT},
        WindowsAndMessaging::{
            WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
        },
    };

    use super::*;
    use crate::test_layout::FakeLayout;

    fn ids(plan: &[Sent]) -> Vec<u32> {
        plan.iter()
            .filter_map(|sent| match sent {
                Sent::Message { id, .. } => Some(*id),
                Sent::Pause(_) => None,
            })
            .collect()
    }

    fn messages(plan: &[Sent]) -> Vec<(u32, usize, isize)> {
        plan.iter()
            .filter_map(|sent| match *sent {
                Sent::Message { id, wparam, lparam } => Some((id, wparam, lparam)),
                Sent::Pause(_) => None,
            })
            .collect()
    }

    fn keys(spec: &str) -> Result<Vec<Sent>> {
        keys_plan(&parse_keys(spec).unwrap(), &FakeLayout)
    }

    #[test]
    fn a_click_sweeps_in_from_the_left_then_presses_and_releases() {
        let plan = click_plan(Point::new(100, 50), MouseButton::Left, 1);
        let sent = messages(&plan);
        let sweep: Vec<isize> = sent.iter().take(5).map(|(_, _, lparam)| *lparam).collect();
        let expected: Vec<isize> = [40, 60, 80, 92, 100].map(|x| point_lparam(Point::new(x, 50))).to_vec();
        assert_eq!(sweep, expected);
        assert!(sent.iter().take(5).all(|(id, wparam, _)| *id == WM_MOUSEMOVE && *wparam == 0));
        let at = point_lparam(Point::new(100, 50));
        assert_eq!(&sent[5..], &[(WM_LBUTTONDOWN, 0x0001, at), (WM_LBUTTONUP, 0, at)]);
        assert!(plan.contains(&Sent::Pause(PRESS_HOLD)));
    }

    #[test]
    fn the_second_click_is_a_double_click_and_the_third_a_plain_press() {
        let double = ids(&click_plan(Point::new(5, 5), MouseButton::Left, 2));
        assert_eq!(&double[5..], &[WM_LBUTTONDOWN, WM_LBUTTONUP, WM_LBUTTONDBLCLK, WM_LBUTTONUP]);
        let triple = ids(&click_plan(Point::new(5, 5), MouseButton::Left, 3));
        assert_eq!(
            &triple[5..],
            &[WM_LBUTTONDOWN, WM_LBUTTONUP, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_LBUTTONDOWN, WM_LBUTTONUP]
        );
    }

    #[test]
    fn right_clicks_use_the_right_button_messages() {
        let sent = messages(&click_plan(Point::new(5, 5), MouseButton::Right, 1));
        assert_eq!(sent[5].0, WM_RBUTTONDOWN);
        assert_eq!(sent[5].1, 0x0002);
        assert_eq!(sent[6].0, WM_RBUTTONUP);
        assert_eq!(release_plan(Point::new(5, 5), MouseButton::Right), message(WM_RBUTTONUP, 0, sent[6].2));
    }

    #[test]
    fn a_drag_holds_the_button_while_it_moves_and_lands_on_the_target() {
        let plan = drag_plan(Point::new(0, 0), Point::new(80, 160), MouseButton::Left, Duration::from_millis(80));
        let sent = messages(&plan);
        assert_eq!(sent[5], (WM_LBUTTONDOWN, 0x0001, point_lparam(Point::new(0, 0))));
        let moves = &sent[6..14];
        assert!(moves.iter().all(|(id, wparam, _)| *id == WM_MOUSEMOVE && *wparam == 0x0001));
        assert_eq!(moves[0].2, point_lparam(Point::new(10, 20)));
        assert_eq!(moves[7].2, point_lparam(Point::new(80, 160)));
        assert_eq!(sent[14], (WM_LBUTTONUP, 0, point_lparam(Point::new(80, 160))));
        assert!(plan.contains(&Sent::Pause(Duration::from_millis(10))));
    }

    #[test]
    fn the_wheel_goes_to_screen_coordinates_one_notch_at_a_time() {
        let plan = scroll_plan(Point::new(5, 6), Point::new(500, 600), Direction::Down, 3).unwrap();
        let sent = messages(&plan);
        assert_eq!(sent[0], (WM_MOUSEMOVE, 0, point_lparam(Point::new(5, 6))));
        assert_eq!(&sent[1..], &[(WM_MOUSEWHEEL, 0xFF88_0000, point_lparam(Point::new(500, 600))); 3]);
        assert!(scroll_plan(Point::new(0, 0), Point::new(0, 0), Direction::Up, 0).is_err());
        let capped = scroll_plan(Point::new(0, 0), Point::new(0, 0), Direction::Up, 500).unwrap();
        assert_eq!(ids(&capped).len(), 1 + MAX_SCROLL_NOTCHES as usize);
    }

    #[test]
    fn text_is_one_char_message_per_utf16_unit_and_newlines_press_enter() {
        let enter = key_code(VK_RETURN, &FakeLayout);
        let sent = messages(&text_plan("a😀\r\nb", enter));
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
        assert!(text_plan("", enter).is_empty());
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
        assert_eq!(keys_plan(&[empty], &FakeLayout).unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn drag_steps_spread_evenly_between_the_ends() {
        assert_eq!(between(Point::new(-40, 10), Point::new(40, 10), 4), Point::new(0, 10));
        assert_eq!(between(Point::new(0, 0), Point::new(7, 7), DRAG_STEPS), Point::new(7, 7));
    }
}
