use std::{
    thread,
    time::{Duration, Instant},
};

use windows::Win32::System::Threading::INFINITE;

const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365);

pub(crate) fn finite_millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX).min(INFINITE - 1)
}

pub(crate) fn later(start: Instant, duration: Duration) -> Instant {
    start.checked_add(duration).or_else(|| start.checked_add(FAR_FUTURE)).unwrap_or(start)
}

pub(crate) fn poll_until(deadline: Instant, interval: Duration, mut done: impl FnMut() -> bool) -> bool {
    loop {
        if done() {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        thread::sleep(interval.min(deadline.saturating_duration_since(now)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_never_reach_infinite() {
        assert_eq!(finite_millis(Duration::from_millis(250)), 250);
        assert_eq!(finite_millis(Duration::MAX), INFINITE - 1);
    }

    #[test]
    fn later_saturates_instead_of_panicking() {
        let now = Instant::now();
        assert!(later(now, Duration::MAX) > now);
        assert_eq!(later(now, Duration::from_millis(5)), now + Duration::from_millis(5));
    }

    #[test]
    fn polling_stops_as_soon_as_the_condition_holds() {
        let mut checks = 0;
        let deadline = later(Instant::now(), Duration::from_secs(5));
        assert!(poll_until(deadline, Duration::from_millis(1), || {
            checks += 1;
            checks == 3
        }));
        assert_eq!(checks, 3);
    }

    #[test]
    fn polling_gives_up_at_the_deadline() {
        let started = Instant::now();
        let deadline = later(started, Duration::from_millis(20));
        assert!(!poll_until(deadline, Duration::from_millis(5), || false));
        assert!(started.elapsed() >= Duration::from_millis(20));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_past_deadline_still_checks_once() {
        let mut checks = 0;
        assert!(!poll_until(Instant::now(), Duration::from_secs(1), || {
            checks += 1;
            false
        }));
        assert_eq!(checks, 1);
    }
}
