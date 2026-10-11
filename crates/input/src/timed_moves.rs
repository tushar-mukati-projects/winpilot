use std::time::{Duration, Instant};

use deckhand_core::{
    Point, Result,
    glide_timing::{capped_glide, glide_path},
};

use crate::{event::InputEvent, mouse::move_event, virtual_desktop::VirtualDesktop};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimedMove {
    pub(crate) after: Duration,
    pub(crate) event: InputEvent,
}

pub(crate) fn move_plan(desktop: VirtualDesktop, from: Point, to: Point, duration: Duration) -> Vec<TimedMove> {
    let duration = capped_glide(duration);
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
    mut guard: impl FnMut() -> Result<()>,
) -> Result<()> {
    let started = Instant::now();
    for step in plan {
        let wait = step.after.saturating_sub(started.elapsed());
        if !wait.is_zero() {
            sleep(wait);
        }
        guard()?;
        send(&[step.event])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Error, ErrorCode, glide_timing::MAX_GLIDE_MS};

    use super::*;

    const DESKTOP: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };

    #[test]
    fn a_plan_steps_at_about_120_hz_and_ends_on_the_target() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(2559, 1599), Duration::from_millis(120));
        assert_eq!(plan.len(), 15);
        assert_eq!(plan.last().map(|step| step.after), Some(Duration::from_millis(120)));
        assert_eq!(plan.last().map(|step| step.event), Some(move_event(DESKTOP, Point::new(2559, 1599))));
        assert!(plan.windows(2).all(|pair| pair[1].after > pair[0].after));
    }

    #[test]
    fn an_instant_plan_is_one_move() {
        let plan = move_plan(DESKTOP, Point::new(5, 5), Point::new(50, 50), Duration::ZERO);
        assert_eq!(plan, vec![TimedMove { after: Duration::ZERO, event: move_event(DESKTOP, Point::new(50, 50)) }]);
    }

    #[test]
    fn long_plans_are_capped_and_keep_their_pace() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(900, 0), Duration::from_secs(60));
        assert_eq!(plan.last().map(|step| step.after), Some(Duration::from_millis(MAX_GLIDE_MS)));
        let gap = plan[1].after.saturating_sub(plan[0].after);
        assert!(gap <= Duration::from_millis(9), "{gap:?}");
    }

    #[test]
    fn playing_sends_every_move_in_order_and_waits_between_them() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), Duration::from_millis(40));
        let mut sent = Vec::new();
        let mut waited = Duration::ZERO;
        let mut checks = 0;
        let sender = |events: &[InputEvent]| {
            sent.extend_from_slice(events);
            Ok(())
        };
        let guard = || {
            checks += 1;
            Ok(())
        };
        play(&plan, |wait| waited += wait, sender, guard).unwrap();
        assert_eq!(sent, plan.iter().map(|step| step.event).collect::<Vec<_>>());
        assert_eq!(checks, plan.len());
        assert!(waited > Duration::from_millis(30));
    }

    #[test]
    fn a_failed_move_stops_the_glide() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(100, 0), Duration::from_millis(40));
        let mut calls = 0;
        let sender = |_: &[InputEvent]| {
            calls += 1;
            if calls == 2 { Err(Error::internal("blocked")) } else { Ok(()) }
        };
        assert!(play(&plan, |_| {}, sender, || Ok(())).is_err());
        assert_eq!(calls, 2);
    }

    #[test]
    fn an_abort_stops_before_the_end_and_sends_nothing_more() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(600, 0), Duration::from_millis(120));
        let mut sent = Vec::new();
        let mut checks = 0;
        let sender = |events: &[InputEvent]| {
            sent.extend_from_slice(events);
            Ok(())
        };
        let guard = || {
            checks += 1;
            if checks > 4 { Err(Error::new(ErrorCode::StoppedByUser, "stopped")) } else { Ok(()) }
        };
        let outcome = play(&plan, |_| {}, sender, guard);
        assert_eq!(outcome.unwrap_err().code, ErrorCode::StoppedByUser);
        assert_eq!(sent, plan.iter().take(4).map(|step| step.event).collect::<Vec<_>>());
        assert!(sent.len() < plan.len());
    }

    #[test]
    fn an_abort_before_the_first_move_sends_nothing() {
        let plan = move_plan(DESKTOP, Point::new(0, 0), Point::new(600, 0), Duration::from_millis(120));
        let mut sent = 0;
        let sender = |_: &[InputEvent]| {
            sent += 1;
            Ok(())
        };
        let outcome = play(&plan, |_| {}, sender, || Err(Error::new(ErrorCode::UserActive, "user moved the mouse")));
        assert_eq!(outcome.unwrap_err().code, ErrorCode::UserActive);
        assert_eq!(sent, 0);
    }
}
