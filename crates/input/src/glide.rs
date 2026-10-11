use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Point, Result, glide::glide_path};

use crate::{
    event::InputEvent,
    mouse::{cursor_position, move_event},
    send::send,
    virtual_desktop::VirtualDesktop,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimedMove {
    pub(crate) after: Duration,
    pub(crate) event: InputEvent,
}

pub fn glide_to(target: Point, duration: Duration) -> Result<()> {
    let from = cursor_position()?;
    let desktop = VirtualDesktop::current()?;
    play(&glide_plan(desktop, from, target, duration), thread::sleep, send)
}

pub(crate) fn glide_plan(desktop: VirtualDesktop, from: Point, to: Point, duration: Duration) -> Vec<TimedMove> {
    let path = glide_path(from, to, duration);
    let steps = u32::try_from(path.len()).unwrap_or(u32::MAX).max(1);
    path.into_iter()
        .enumerate()
        .map(|(index, point)| {
            let step = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            TimedMove { after: duration.mul_f64(f64::from(step) / f64::from(steps)), event: move_event(desktop, point) }
        })
        .collect()
}

pub(crate) fn play(
    plan: &[TimedMove],
    mut sleep: impl FnMut(Duration),
    mut send: impl FnMut(&[InputEvent]) -> Result<()>,
) -> Result<()> {
    let started = Instant::now();
    for step in plan {
        let wait = step.after.saturating_sub(started.elapsed());
        if !wait.is_zero() {
            sleep(wait);
        }
        send(&[step.event])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use deckhand_core::Error;

    use super::*;

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };

    #[test]
    fn a_glide_steps_at_about_120_hz_and_ends_on_the_target() {
        let plan = glide_plan(DESKTOP, Point::new(0, 0), Point::new(2559, 1599), Duration::from_millis(120));
        assert_eq!(plan.len(), 15);
        assert_eq!(plan.last().map(|step| step.after), Some(Duration::from_millis(120)));
        assert_eq!(plan.last().map(|step| step.event), Some(move_event(DESKTOP, Point::new(2559, 1599))));
        assert!(plan.windows(2).all(|pair| pair[1].after > pair[0].after));
    }

    #[test]
    fn an_instant_glide_is_one_move() {
        let plan = glide_plan(DESKTOP, Point::new(5, 5), Point::new(50, 50), Duration::ZERO);
        assert_eq!(plan, vec![TimedMove { after: Duration::ZERO, event: move_event(DESKTOP, Point::new(50, 50)) }]);
    }

    #[test]
    fn playing_sends_every_move_in_order_and_waits_between_them() {
        let plan = glide_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), Duration::from_millis(40));
        let mut sent = Vec::new();
        let mut waited = Duration::ZERO;
        play(
            &plan,
            |wait| waited += wait,
            |events| {
                sent.extend_from_slice(events);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(sent, plan.iter().map(|step| step.event).collect::<Vec<_>>());
        assert!(waited > Duration::from_millis(30));
    }

    #[test]
    fn a_failed_move_stops_the_glide() {
        let plan = glide_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), Duration::from_millis(40));
        let mut calls = 0;
        let outcome = play(
            &plan,
            |_| {},
            |_| {
                calls += 1;
                if calls == 2 { Err(Error::internal("blocked")) } else { Ok(()) }
            },
        );
        assert!(outcome.is_err());
        assert_eq!(calls, 2);
    }
}
