use std::{thread, time::Duration};

use deckhand_core::{Point, Result, protocol::MouseButton};

use crate::{
    event::InputEvent,
    mouse::{ButtonTransition, button_event, move_event},
    physical_button::PhysicalButton,
    send::send,
    timed_moves::{move_plan, play},
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

struct DragIo<Sleep, Send, Guard> {
    sleep: Sleep,
    send: Send,
    guard: Guard,
}

pub fn drag_guarded(
    from: Point,
    to: Point,
    button: MouseButton,
    duration: Duration,
    guard: impl FnMut() -> Result<()>,
) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    let plan = DragPlan { desktop, from, to, button: PhysicalButton::current(button), duration };
    drag_with(plan, DragIo { sleep: thread::sleep, send, guard })
}

pub fn drag(from: Point, to: Point, button: MouseButton, duration: Duration) -> Result<()> {
    drag_guarded(from, to, button, duration, || Ok(()))
}

fn drag_with(
    plan: DragPlan,
    mut io: DragIo<impl FnMut(Duration), impl FnMut(&[InputEvent]) -> Result<()>, impl FnMut() -> Result<()>>,
) -> Result<()> {
    (io.guard)()?;
    let outcome = perform_drag(plan, &mut io);
    if outcome.is_err() {
        let _ = (io.send)(&[button_event(plan.button, ButtonTransition::Release)]);
    }
    outcome
}

fn perform_drag(
    plan: DragPlan,
    io: &mut DragIo<impl FnMut(Duration), impl FnMut(&[InputEvent]) -> Result<()>, impl FnMut() -> Result<()>>,
) -> Result<()> {
    (io.send)(&[move_event(plan.desktop, plan.from), button_event(plan.button, ButtonTransition::Press)])?;
    let moves = move_plan(plan.desktop, plan.from, plan.to, plan.duration.max(SHORTEST_DRAG));
    play(&moves, &mut io.sleep, &mut io.send, &mut io.guard)?;
    (io.send)(&[button_event(plan.button, ButtonTransition::Release)])
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Error, ErrorCode};

    use super::*;

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };
    const FROM: Point = Point::new(10, 10);
    const TO: Point = Point::new(200, 300);

    fn plan(button: PhysicalButton) -> DragPlan {
        DragPlan { desktop: DESKTOP, from: FROM, to: TO, button, duration: Duration::ZERO }
    }

    fn recorded_drag(
        button: PhysicalButton,
        fail_on_call: Option<usize>,
        abort_on_check: Option<usize>,
    ) -> (Result<()>, Vec<Vec<InputEvent>>) {
        let mut calls: Vec<Vec<InputEvent>> = Vec::new();
        let mut checks = 0;
        let send = |events: &[InputEvent]| {
            calls.push(events.to_vec());
            if Some(calls.len()) == fail_on_call { Err(Error::internal("blocked")) } else { Ok(()) }
        };
        let guard = || {
            checks += 1;
            if Some(checks) == abort_on_check { Err(Error::new(ErrorCode::StoppedByUser, "stopped")) } else { Ok(()) }
        };
        let outcome = drag_with(plan(button), DragIo { sleep: |_| {}, send, guard });
        (outcome, calls)
    }

    fn release(button: PhysicalButton) -> Vec<InputEvent> {
        vec![button_event(button, ButtonTransition::Release)]
    }

    fn path_moves() -> usize {
        move_plan(DESKTOP, FROM, TO, SHORTEST_DRAG).len()
    }

    #[test]
    fn successful_drag_presses_glides_to_the_target_and_releases() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, None, None);
        assert!(outcome.is_ok());
        assert_eq!(calls.len(), path_moves() + 2);
        assert_eq!(
            calls[0],
            vec![move_event(DESKTOP, FROM), button_event(PhysicalButton::Left, ButtonTransition::Press)]
        );
        assert_eq!(calls[calls.len() - 2], vec![move_event(DESKTOP, TO)]);
        assert_eq!(calls.last(), Some(&release(PhysicalButton::Left)));
    }

    #[test]
    fn failed_intermediate_move_still_releases_the_button() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Right, Some(4), None);
        assert!(outcome.is_err());
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[4], release(PhysicalButton::Right));
    }

    #[test]
    fn failed_press_still_sends_a_release() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, Some(1), None);
        assert!(outcome.is_err());
        assert_eq!(calls.last(), Some(&release(PhysicalButton::Left)));
    }

    #[test]
    fn an_abort_mid_drag_stops_moving_and_releases_the_button() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, None, Some(5));
        assert_eq!(outcome.unwrap_err().code, ErrorCode::StoppedByUser);
        assert_eq!(calls.len(), 1 + 3 + 1);
        assert!(calls.len() < path_moves() + 2);
        assert_eq!(calls.last(), Some(&release(PhysicalButton::Left)));
    }

    #[test]
    fn an_abort_before_the_press_sends_nothing() {
        let (outcome, calls) = recorded_drag(PhysicalButton::Left, None, Some(1));
        assert_eq!(outcome.unwrap_err().code, ErrorCode::StoppedByUser);
        assert!(calls.is_empty());
    }

    #[test]
    fn a_drag_takes_at_least_a_tenth_of_a_second_and_eases() {
        let quick = move_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), SHORTEST_DRAG);
        assert_eq!(quick.len(), 12);
        assert_eq!(quick.last().map(|step| step.after), Some(SHORTEST_DRAG));
        let slow = move_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), Duration::from_millis(500));
        assert_eq!(slow.len(), 60);
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let plan = DragPlan {
            desktop: DESKTOP,
            from: Point::new(i32::MIN, i32::MAX),
            to: Point::new(i32::MAX, i32::MIN),
            button: PhysicalButton::Left,
            duration: Duration::ZERO,
        };
        let mut sent = 0;
        let counting = |_: &[InputEvent]| {
            sent += 1;
            Ok(())
        };
        assert!(drag_with(plan, DragIo { sleep: |_| {}, send: counting, guard: || Ok(()) }).is_ok());
        assert_eq!(sent, path_moves() + 2);
    }
}
