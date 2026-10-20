use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE;
use winpilot_core::{
    Error, Point, Result,
    protocol::{Direction, MouseButton},
};

use crate::{
    message_packing::{button_messages, point_lparam, wheel, wheel_wparam},
    message_plan::{Sent, message},
    mouse::MAX_SCROLL_NOTCHES,
};

const SWEEP_OFFSETS: [i32; 5] = [60, 40, 20, 8, 0];
const SWEEP_PAUSE: Duration = Duration::from_millis(80);
const PRESS_HOLD: Duration = Duration::from_millis(120);
const CLICK_GAP: Duration = Duration::from_millis(60);
const WHEEL_PAUSE: Duration = Duration::from_millis(30);
const DRAG_STEPS: u32 = 8;
const NOTHING_HELD: usize = 0;

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
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    use super::*;
    use crate::message_plan::{ids, messages};

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
    fn drag_steps_spread_evenly_between_the_ends() {
        assert_eq!(between(Point::new(-40, 10), Point::new(40, 10), 4), Point::new(0, 10));
        assert_eq!(between(Point::new(0, 0), Point::new(7, 7), DRAG_STEPS), Point::new(7, 7));
    }
}
