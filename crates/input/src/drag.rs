use std::{thread, time::Duration};

use deckhand_core::{Point, Result, glide::glide_path, protocol::MouseButton};

use crate::{
    event::InputEvent,
    mouse::{ButtonTransition, button_event, move_event},
    physical_button::PhysicalButton,
    send::send,
    virtual_desktop::VirtualDesktop,
};

const SHORTEST_DRAG: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
struct DragPlan {
    desktop: VirtualDesktop,
    from: Point,
    to: Point,
    button: PhysicalButton,
    duration: Duration,
}

pub fn drag(from: Point, to: Point, button: MouseButton, duration: Duration) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    let plan = DragPlan { desktop, from, to, button: PhysicalButton::current(button), duration };
    drag_with(plan, thread::sleep, send)
}

fn drag_with(
    plan: DragPlan,
    mut sleep: impl FnMut(Duration),
    mut send: impl FnMut(&[InputEvent]) -> Result<()>,
) -> Result<()> {
    let outcome = perform_drag(plan, &mut sleep, &mut send);
    if outcome.is_err() {
        let _ = send(&[button_event(plan.button, ButtonTransition::Release)]);
    }
    outcome
}

fn perform_drag(
    plan: DragPlan,
    sleep: &mut impl FnMut(Duration),
    send: &mut impl FnMut(&[InputEvent]) -> Result<()>,
) -> Result<()> {
    send(&[move_event(plan.desktop, plan.from), button_event(plan.button, ButtonTransition::Press)])?;
    let path = drag_path(plan.from, plan.to, plan.duration);
    let delay = step_delay(plan.duration, path.len());
    for point in path {
        sleep(delay);
        send(&[move_event(plan.desktop, point)])?;
    }
    sleep(delay);
    send(&[move_event(plan.desktop, plan.to), button_event(plan.button, ButtonTransition::Release)])
}

fn drag_path(from: Point, to: Point, duration: Duration) -> Vec<Point> {
    let mut path = glide_path(from, to, duration.max(SHORTEST_DRAG));
    path.pop();
    path
}

fn step_delay(duration: Duration, moves: usize) -> Duration {
    let steps = u32::try_from(moves).unwrap_or(u32::MAX).saturating_add(1);
    duration.max(SHORTEST_DRAG) / steps
}

#[cfg(test)]
mod tests {
    use deckhand_core::Error;

    use super::*;

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };

    fn plan(button: PhysicalButton) -> DragPlan {
        DragPlan {
            desktop: DESKTOP,
            from: Point::new(10, 10),
            to: Point::new(200, 300),
            button,
            duration: Duration::ZERO,
        }
    }

    fn recorded_drag(button: PhysicalButton, fail_on_call: Option<usize>) -> (Result<()>, Vec<Vec<InputEvent>>) {
        let mut calls: Vec<Vec<InputEvent>> = Vec::new();
        let outcome = drag_with(
            plan(button),
            |_| {},
            |events: &[InputEvent]| {
                calls.push(events.to_vec());
                if Some(calls.len()) == fail_on_call { Err(Error::internal("blocked")) } else { Ok(()) }
            },
        );
        (outcome, calls)
    }

    #[test]
    fn successful_drag_presses_moves_and_releases() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, None);
        assert!(outcome.is_ok());
        assert_eq!(calls.len(), drag_path(Point::new(10, 10), Point::new(200, 300), Duration::ZERO).len() + 2);
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
    fn the_drag_path_eases_and_takes_at_least_a_tenth_of_a_second() {
        let quick = drag_path(Point::new(0, 0), Point::new(100, 0), Duration::ZERO);
        assert_eq!(quick.len(), 11);
        assert!(quick[1].x - quick[0].x < quick[6].x - quick[5].x);
        assert_eq!(drag_path(Point::new(0, 0), Point::new(100, 0), Duration::from_millis(500)).len(), 59);
        assert_eq!(step_delay(Duration::ZERO, 11), Duration::from_millis(100) / 12);
    }

    #[test]
    fn zero_length_drag_stays_put() {
        let path = drag_path(Point::new(5, 5), Point::new(5, 5), Duration::ZERO);
        assert!(path.iter().all(|point| *point == Point::new(5, 5)));
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let path = drag_path(Point::new(i32::MIN, i32::MAX), Point::new(i32::MAX, i32::MIN), Duration::ZERO);
        assert_eq!(path.len(), 11);
        assert!(path[0].x < 0 && path[10].x > 0);
    }
}
