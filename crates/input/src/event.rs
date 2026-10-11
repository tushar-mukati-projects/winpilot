use deckhand_core::INJECTION_MARK;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, MOUSE_EVENT_FLAGS, MOUSEINPUT,
    VIRTUAL_KEY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub vk: VIRTUAL_KEY,
    pub scan: u16,
    pub flags: KEYBD_EVENT_FLAGS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseAction {
    pub dx: i32,
    pub dy: i32,
    pub data: u32,
    pub flags: MOUSE_EVENT_FLAGS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyStroke),
    Mouse(MouseAction),
}

impl InputEvent {
    pub fn to_win32(self) -> INPUT {
        match self {
            Self::Key(stroke) => keyboard_input(stroke),
            Self::Mouse(action) => mouse_input(action),
        }
    }
}

fn keyboard_input(stroke: KeyStroke) -> INPUT {
    let ki =
        KEYBDINPUT { wVk: stroke.vk, wScan: stroke.scan, dwFlags: stroke.flags, time: 0, dwExtraInfo: INJECTION_MARK };
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }
}

fn mouse_input(action: MouseAction) -> INPUT {
    let mi = MOUSEINPUT {
        dx: action.dx,
        dy: action.dy,
        mouseData: action.data,
        dwFlags: action.flags,
        time: 0,
        dwExtraInfo: INJECTION_MARK,
    };
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{
        Point,
        keys::{Chord, Key, Modifiers, NamedKey},
        protocol::Direction,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_UNICODE, MOUSEEVENTF_MOVE, VK_RETURN};

    use super::*;
    use crate::{
        chord::chord_batch, mouse::click_batch, mouse::scroll_events, physical_button::PhysicalButton,
        test_layout::FakeLayout, text::text_events, virtual_desktop::VirtualDesktop,
    };

    fn extra_info(input: &INPUT) -> usize {
        if input.r#type == INPUT_KEYBOARD {
            unsafe { input.Anonymous.ki.dwExtraInfo }
        } else {
            unsafe { input.Anonymous.mi.dwExtraInfo }
        }
    }

    fn every_kind_of_event() -> Vec<InputEvent> {
        let layout = FakeLayout;
        let desktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let chord = Chord { modifiers: ctrl, key: Some(Key::Named(NamedKey::Left)) };
        let mut events =
            click_batch(desktop, Point::new(5, 5), PhysicalButton::Right, 2, ctrl, &layout).unwrap().events;
        events.extend(scroll_events(desktop, Point::new(5, 5), Direction::Left, 3).unwrap());
        events.extend(chord_batch(&chord, &layout).unwrap().events);
        events.extend(text_events("a\nb\t😀", &layout));
        events
    }

    #[test]
    fn every_input_carries_the_injection_mark() {
        let events = every_kind_of_event();
        assert!(events.len() > 20);
        for input in events.into_iter().map(InputEvent::to_win32) {
            assert_eq!(extra_info(&input), INJECTION_MARK);
        }
    }

    #[test]
    fn key_stroke_converts_field_by_field() {
        let stroke = KeyStroke { vk: VK_RETURN, scan: 0x1C, flags: KEYEVENTF_UNICODE };
        let input = InputEvent::Key(stroke).to_win32();
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        let ki = unsafe { input.Anonymous.ki };
        assert_eq!((ki.wVk, ki.wScan, ki.dwFlags, ki.time), (VK_RETURN, 0x1C, KEYEVENTF_UNICODE, 0));
    }

    #[test]
    fn mouse_action_converts_field_by_field() {
        let action = MouseAction { dx: 10, dy: -20, data: 120, flags: MOUSEEVENTF_MOVE };
        let input = InputEvent::Mouse(action).to_win32();
        assert_eq!(input.r#type, INPUT_MOUSE);
        let mi = unsafe { input.Anonymous.mi };
        assert_eq!((mi.dx, mi.dy, mi.mouseData, mi.dwFlags, mi.time), (10, -20, 120, MOUSEEVENTF_MOVE, 0));
    }
}
