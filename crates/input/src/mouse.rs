use deckhand_core::{
    Context, Error, Point, Result,
    keys::Modifiers,
    protocol::{Direction, MouseButton},
};
use windows::Win32::{
    Foundation::POINT,
    UI::{
        Input::KeyboardAndMouse::{
            MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
            MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
        },
        WindowsAndMessaging::{GetCursorPos, WHEEL_DELTA},
    },
};

use crate::{
    batch::{Batch, with_modifiers_held},
    event::{InputEvent, MouseAction},
    layout::{KeyboardLayout, SystemLayout},
    physical_button::PhysicalButton,
    send::{send, send_batch},
    virtual_desktop::VirtualDesktop,
};

const CLICK_COUNTS: std::ops::RangeInclusive<u8> = 1..=3;
const MAX_SCROLL_NOTCHES: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTransition {
    Press,
    Release,
}

pub fn move_to(point: Point) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    send(&[move_event(desktop, point)])
}

pub fn click(point: Point, button: MouseButton, count: u8, modifiers: Modifiers) -> Result<()> {
    validate_click_count(count)?;
    let desktop = VirtualDesktop::current()?;
    let layout = SystemLayout::of_foreground_window();
    send_batch(&click_batch(desktop, point, PhysicalButton::current(button), count, modifiers, &layout)?)
}

pub fn button_down(point: Point, button: MouseButton) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    send(&button_events(desktop, point, PhysicalButton::current(button), ButtonTransition::Press))
}

pub fn button_up(point: Point, button: MouseButton) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    send(&button_events(desktop, point, PhysicalButton::current(button), ButtonTransition::Release))
}

pub fn button_events(
    desktop: VirtualDesktop,
    point: Point,
    button: PhysicalButton,
    transition: ButtonTransition,
) -> Vec<InputEvent> {
    vec![move_event(desktop, point), button_event(button, transition)]
}

pub fn scroll(point: Point, direction: Direction, notches: u32) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    send(&scroll_events(desktop, point, direction, notches)?)
}

pub fn cursor_position() -> Result<Point> {
    let mut position = POINT::default();
    unsafe { GetCursorPos(&raw mut position) }.context("reading the cursor position")?;
    Ok(Point::new(position.x, position.y))
}

pub fn move_event(desktop: VirtualDesktop, point: Point) -> InputEvent {
    let (dx, dy) = desktop.absolute(point);
    let flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
    InputEvent::Mouse(MouseAction { dx, dy, data: 0, flags })
}

pub fn button_event(button: PhysicalButton, transition: ButtonTransition) -> InputEvent {
    InputEvent::Mouse(MouseAction { dx: 0, dy: 0, data: 0, flags: button_flags(button, transition) })
}

pub fn click_batch(
    desktop: VirtualDesktop,
    point: Point,
    button: PhysicalButton,
    count: u8,
    modifiers: Modifiers,
    layout: &impl KeyboardLayout,
) -> Result<Batch> {
    validate_click_count(count)?;
    let mut inner = vec![move_event(desktop, point)];
    for _ in 0..count {
        inner.push(button_event(button, ButtonTransition::Press));
        inner.push(button_event(button, ButtonTransition::Release));
    }
    let mut batch = with_modifiers_held(modifiers, inner, layout);
    batch.release_on_failure.insert(0, button_event(button, ButtonTransition::Release));
    Ok(batch)
}

pub fn scroll_events(
    desktop: VirtualDesktop,
    point: Point,
    direction: Direction,
    notches: u32,
) -> Result<Vec<InputEvent>> {
    if notches == 0 {
        return Err(Error::invalid("scroll needs at least one notch"));
    }
    Ok(vec![move_event(desktop, point), wheel_event(direction, notches.min(MAX_SCROLL_NOTCHES))])
}

fn validate_click_count(count: u8) -> Result<()> {
    if CLICK_COUNTS.contains(&count) {
        return Ok(());
    }
    Err(Error::invalid(format!("click count must be 1, 2 or 3, not {count}")))
}

fn wheel_event(direction: Direction, notches: u32) -> InputEvent {
    let delta = (WHEEL_DELTA * notches) as i32;
    let (flags, amount) = match direction {
        Direction::Up => (MOUSEEVENTF_WHEEL, delta),
        Direction::Down => (MOUSEEVENTF_WHEEL, -delta),
        Direction::Right => (MOUSEEVENTF_HWHEEL, delta),
        Direction::Left => (MOUSEEVENTF_HWHEEL, -delta),
    };
    InputEvent::Mouse(MouseAction { dx: 0, dy: 0, data: amount.cast_unsigned(), flags })
}

fn button_flags(button: PhysicalButton, transition: ButtonTransition) -> MOUSE_EVENT_FLAGS {
    match (button, transition) {
        (PhysicalButton::Left, ButtonTransition::Press) => MOUSEEVENTF_LEFTDOWN,
        (PhysicalButton::Left, ButtonTransition::Release) => MOUSEEVENTF_LEFTUP,
        (PhysicalButton::Right, ButtonTransition::Press) => MOUSEEVENTF_RIGHTDOWN,
        (PhysicalButton::Right, ButtonTransition::Release) => MOUSEEVENTF_RIGHTUP,
        (PhysicalButton::Middle, ButtonTransition::Press) => MOUSEEVENTF_MIDDLEDOWN,
        (PhysicalButton::Middle, ButtonTransition::Release) => MOUSEEVENTF_MIDDLEUP,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    use super::*;
    use crate::{event::KeyStroke, test_layout::FakeLayout};

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };
    const ABSOLUTE_MOVE: MOUSE_EVENT_FLAGS =
        MOUSE_EVENT_FLAGS(MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0);

    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        Key(VIRTUAL_KEY, KEYBD_EVENT_FLAGS),
        Mouse(MOUSE_EVENT_FLAGS),
    }

    fn seen(events: &[InputEvent]) -> Vec<Seen> {
        events
            .iter()
            .map(|event| match *event {
                InputEvent::Key(KeyStroke { vk, flags, .. }) => Seen::Key(vk, flags),
                InputEvent::Mouse(action) => Seen::Mouse(action.flags),
            })
            .collect()
    }

    fn mouse(event: InputEvent) -> MouseAction {
        match event {
            InputEvent::Mouse(action) => action,
            InputEvent::Key(stroke) => panic!("unexpected key event {stroke:?}"),
        }
    }

    #[test]
    fn move_is_absolute_over_the_virtual_desktop() {
        let action = mouse(move_event(DESKTOP, Point::new(2559, 0)));
        assert_eq!(action, MouseAction { dx: 65_523, dy: 20, data: 0, flags: ABSOLUTE_MOVE });
    }

    #[test]
    fn single_left_click_without_modifiers() {
        let batch =
            click_batch(DESKTOP, Point::new(10, 10), PhysicalButton::Left, 1, Modifiers::NONE, &FakeLayout).unwrap();
        assert_eq!(
            seen(&batch.events),
            vec![Seen::Mouse(ABSOLUTE_MOVE), Seen::Mouse(MOUSEEVENTF_LEFTDOWN), Seen::Mouse(MOUSEEVENTF_LEFTUP)]
        );
        assert_eq!(seen(&batch.release_on_failure), vec![Seen::Mouse(MOUSEEVENTF_LEFTUP)]);
    }

    #[test]
    fn double_click_with_ctrl_shift_holds_modifiers_around_the_clicks() {
        let modifiers = Modifiers { ctrl: true, shift: true, ..Modifiers::NONE };
        let batch = click_batch(DESKTOP, Point::new(10, 10), PhysicalButton::Left, 2, modifiers, &FakeLayout).unwrap();
        let down = KEYBD_EVENT_FLAGS::default();
        assert_eq!(
            seen(&batch.events),
            vec![
                Seen::Key(VK_CONTROL, down),
                Seen::Key(VK_SHIFT, down),
                Seen::Mouse(ABSOLUTE_MOVE),
                Seen::Mouse(MOUSEEVENTF_LEFTDOWN),
                Seen::Mouse(MOUSEEVENTF_LEFTUP),
                Seen::Mouse(MOUSEEVENTF_LEFTDOWN),
                Seen::Mouse(MOUSEEVENTF_LEFTUP),
                Seen::Key(VK_SHIFT, KEYEVENTF_KEYUP),
                Seen::Key(VK_CONTROL, KEYEVENTF_KEYUP),
            ]
        );
        assert_eq!(
            seen(&batch.release_on_failure),
            vec![
                Seen::Mouse(MOUSEEVENTF_LEFTUP),
                Seen::Key(VK_SHIFT, KEYEVENTF_KEYUP),
                Seen::Key(VK_CONTROL, KEYEVENTF_KEYUP)
            ]
        );
    }

    #[test]
    fn triple_right_click_with_alt() {
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        let batch = click_batch(DESKTOP, Point::new(0, 0), PhysicalButton::Right, 3, alt, &FakeLayout).unwrap();
        let events = seen(&batch.events);
        assert_eq!(events.len(), 9);
        assert_eq!(events[0], Seen::Key(VK_MENU, KEYBD_EVENT_FLAGS::default()));
        assert_eq!(events.iter().filter(|e| **e == Seen::Mouse(MOUSEEVENTF_RIGHTDOWN)).count(), 3);
        assert_eq!(events.iter().filter(|e| **e == Seen::Mouse(MOUSEEVENTF_RIGHTUP)).count(), 3);
        assert_eq!(events[8], Seen::Key(VK_MENU, KEYEVENTF_KEYUP));
    }

    #[test]
    fn middle_button_uses_middle_flags() {
        let batch =
            click_batch(DESKTOP, Point::new(0, 0), PhysicalButton::Middle, 1, Modifiers::NONE, &FakeLayout).unwrap();
        assert_eq!(seen(&batch.events)[1..], [Seen::Mouse(MOUSEEVENTF_MIDDLEDOWN), Seen::Mouse(MOUSEEVENTF_MIDDLEUP)]);
    }

    #[test]
    fn primary_click_uses_the_left_button_when_buttons_are_not_swapped() {
        let button = PhysicalButton::of(MouseButton::Left, false);
        let batch = click_batch(DESKTOP, Point::new(0, 0), button, 1, Modifiers::NONE, &FakeLayout).unwrap();
        assert_eq!(seen(&batch.events)[1..], [Seen::Mouse(MOUSEEVENTF_LEFTDOWN), Seen::Mouse(MOUSEEVENTF_LEFTUP)]);
        assert_eq!(seen(&batch.release_on_failure), vec![Seen::Mouse(MOUSEEVENTF_LEFTUP)]);
    }

    #[test]
    fn primary_click_uses_the_right_button_when_buttons_are_swapped() {
        let button = PhysicalButton::of(MouseButton::Left, true);
        let batch = click_batch(DESKTOP, Point::new(0, 0), button, 1, Modifiers::NONE, &FakeLayout).unwrap();
        assert_eq!(seen(&batch.events)[1..], [Seen::Mouse(MOUSEEVENTF_RIGHTDOWN), Seen::Mouse(MOUSEEVENTF_RIGHTUP)]);
        assert_eq!(seen(&batch.release_on_failure), vec![Seen::Mouse(MOUSEEVENTF_RIGHTUP)]);
    }

    #[test]
    fn click_count_outside_one_to_three_is_rejected() {
        for count in [0, 4, 255] {
            let error =
                click_batch(DESKTOP, Point::new(0, 0), PhysicalButton::Left, count, Modifiers::NONE, &FakeLayout)
                    .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument);
        }
    }

    #[test]
    fn button_down_and_up_move_first_and_honour_swapped_buttons() {
        let down = button_events(
            DESKTOP,
            Point::new(10, 20),
            PhysicalButton::of(MouseButton::Left, false),
            ButtonTransition::Press,
        );
        assert_eq!(seen(&down), vec![Seen::Mouse(ABSOLUTE_MOVE), Seen::Mouse(MOUSEEVENTF_LEFTDOWN)]);
        let swapped = PhysicalButton::of(MouseButton::Left, true);
        let up = button_events(DESKTOP, Point::new(10, 20), swapped, ButtonTransition::Release);
        assert_eq!(seen(&up), vec![Seen::Mouse(ABSOLUTE_MOVE), Seen::Mouse(MOUSEEVENTF_RIGHTUP)]);
        let middle = button_events(DESKTOP, Point::new(0, 0), PhysicalButton::Middle, ButtonTransition::Press);
        assert_eq!(seen(&middle)[1], Seen::Mouse(MOUSEEVENTF_MIDDLEDOWN));
    }

    #[test]
    fn scroll_moves_first_then_one_wheel_event() {
        let events = scroll_events(DESKTOP, Point::new(1280, 800), Direction::Up, 3).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(mouse(events[0]).flags, ABSOLUTE_MOVE);
        assert_eq!(mouse(events[1]), MouseAction { dx: 0, dy: 0, data: 360, flags: MOUSEEVENTF_WHEEL });
    }

    #[test]
    fn scroll_down_is_negative() {
        let events = scroll_events(DESKTOP, Point::new(0, 0), Direction::Down, 2).unwrap();
        let wheel = mouse(events[1]);
        assert_eq!(wheel.flags, MOUSEEVENTF_WHEEL);
        assert_eq!(wheel.data.cast_signed(), -240);
    }

    #[test]
    fn horizontal_scroll_uses_hwheel_with_right_positive() {
        let right = mouse(scroll_events(DESKTOP, Point::new(0, 0), Direction::Right, 1).unwrap()[1]);
        let left = mouse(scroll_events(DESKTOP, Point::new(0, 0), Direction::Left, 1).unwrap()[1]);
        assert_eq!((right.flags, right.data.cast_signed()), (MOUSEEVENTF_HWHEEL, 120));
        assert_eq!((left.flags, left.data.cast_signed()), (MOUSEEVENTF_HWHEEL, -120));
    }

    #[test]
    fn scroll_is_capped_at_fifty_notches() {
        let wheel = mouse(scroll_events(DESKTOP, Point::new(0, 0), Direction::Down, u32::MAX).unwrap()[1]);
        assert_eq!(wheel.data.cast_signed(), -6_000);
    }

    #[test]
    fn zero_notches_is_rejected() {
        let error = scroll_events(DESKTOP, Point::new(0, 0), Direction::Up, 0).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
}
