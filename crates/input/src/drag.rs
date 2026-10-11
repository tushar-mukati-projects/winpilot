use std::{thread, time::Duration};

use deckhand_core::{Point, Result, protocol::MouseButton};

use crate::{
    event::InputEvent,
    mouse::{ButtonTransition, button_event, move_event},
    physical_button::PhysicalButton,
    send::send,
    virtual_desktop::VirtualDesktop,
};

const DRAG_STEPS: u32 = 12;
const DRAG_STEP_DELAY: Duration = Duration::from_millis(8);

#[derive(Debug, Clone, Copy)]
struct DragPlan {
    desktop: VirtualDesktop,
    from: Point,
    to: Point,
    button: PhysicalButton,
}

pub fn drag(from: Point, to: Point, button: MouseButton) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    let plan = DragPlan { desktop, from, to, button: PhysicalButton::current(button) };
    drag_with(plan, DRAG_STEP_DELAY, send)
}

fn drag_with(plan: DragPlan, step_delay: Duration, mut send: impl FnMut(&[InputEvent]) -> Result<()>) -> Result<()> {
    let outcome = perform_drag(plan, step_delay, &mut send);
    if outcome.is_err() {
        let _ = send(&[button_event(plan.button, ButtonTransition::Release)]);
    }
    outcome
}

fn perform_drag(
    plan: DragPlan,
    step_delay: Duration,
    send: &mut impl FnMut(&[InputEvent]) -> Result<()>,
) -> Result<()> {
    send(&[move_event(plan.desktop, plan.from), button_event(plan.button, ButtonTransition::Press)])?;
    for point in drag_path(plan.from, plan.to) {
        thread::sleep(step_delay);
        send(&[move_event(plan.desktop, point)])?;
    }
    thread::sleep(step_delay);
    send(&[move_event(plan.desktop, plan.to), button_event(plan.button, ButtonTransition::Release)])
}

fn drag_path(from: Point, to: Point) -> Vec<Point> {
    let segments = f64::from(DRAG_STEPS + 1);
    (1..=DRAG_STEPS).map(|step| interpolate(from, to, f64::from(step) / segments)).collect()
}

fn interpolate(from: Point, to: Point, fraction: f64) -> Point {
    Point::new(lerp(from.x, to.x, fraction), lerp(from.y, to.y, fraction))
}

fn lerp(start: i32, end: i32, fraction: f64) -> i32 {
    (f64::from(start) + (f64::from(end) - f64::from(start)) * fraction).round() as i32
}

#[cfg(test)]
mod tests {
    use deckhand_core::Error;

    use super::*;

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };

    fn plan(button: PhysicalButton) -> DragPlan {
        DragPlan { desktop: DESKTOP, from: Point::new(10, 10), to: Point::new(200, 300), button }
    }

    fn recorded_drag(button: PhysicalButton, fail_on_call: Option<usize>) -> (Result<()>, Vec<Vec<InputEvent>>) {
        let mut calls: Vec<Vec<InputEvent>> = Vec::new();
        let outcome = drag_with(plan(button), Duration::ZERO, |events: &[InputEvent]| {
            calls.push(events.to_vec());
            if Some(calls.len()) == fail_on_call { Err(Error::internal("blocked")) } else { Ok(()) }
        });
        (outcome, calls)
    }

    #[test]
    fn successful_drag_presses_moves_and_releases() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, None);
        assert!(outcome.is_ok());
        assert_eq!(calls.len(), DRAG_STEPS as usize + 2);
        assert_eq!(calls[0][1], button_event(PhysicalButton::Left, ButtonTransition::Press));
        let last = calls.last().unwrap();
        assert_eq!(last[1], button_event(PhysicalButton::Left, ButtonTransition::Release));
    }

    #[test]
    fn failed_intermediate_move_still_releases_the_button() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Right, Some(4));
        assert!(outcome.is_err());
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[4], vec![button_event(PhysicalButton::Right, ButtonTransition::Release)]);
    }

    #[test]
    fn failed_press_still_sends_a_release() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, Some(1));
        assert!(outcome.is_err());
        assert_eq!(calls.last().unwrap(), &vec![button_event(PhysicalButton::Left, ButtonTransition::Release)]);
    }

    #[test]
    fn path_has_twelve_points_strictly_between_the_ends() {
        let path = drag_path(Point::new(0, 0), Point::new(130, 260));
        assert_eq!(path.len(), 12);
        assert_eq!(path[0], Point::new(10, 20));
        assert_eq!(path[11], Point::new(120, 240));
        assert!(path.windows(2).all(|pair| pair[0].x < pair[1].x && pair[0].y < pair[1].y));
    }

    #[test]
    fn path_runs_backwards_across_negative_coordinates() {
        let path = drag_path(Point::new(100, 0), Point::new(-1720, -1300));
        assert_eq!(path[0], Point::new(-40, -100));
        assert_eq!(path[11], Point::new(-1580, -1200));
    }

    #[test]
    fn zero_length_drag_stays_put() {
        let path = drag_path(Point::new(5, 5), Point::new(5, 5));
        assert!(path.iter().all(|point| *point == Point::new(5, 5)));
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let path = drag_path(Point::new(i32::MIN, i32::MAX), Point::new(i32::MAX, i32::MIN));
        assert_eq!(path.len(), 12);
        assert!(path[0].x < 0 && path[11].x > 0);
    }
}
