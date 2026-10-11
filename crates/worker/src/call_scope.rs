use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deckhand_core::{Error, Result};

use crate::cancel::Cancel;

const SLACK: Duration = Duration::from_secs(1);
const OUT_OF_TIME: &str =
    "the call ran out of time before it finished; try again with fewer steps or a shorter timeout";

pub struct CallScope {
    cancel: Cancel,
    deadline: Instant,
    acting: AtomicBool,
    overlay_shown: AtomicBool,
}

impl CallScope {
    pub fn new(cancel: Cancel, deadline: Instant) -> Self {
        Self { cancel, deadline, acting: AtomicBool::new(false), overlay_shown: AtomicBool::new(false) }
    }

    pub fn check(&self) -> Result<()> {
        self.cancel.check()?;
        if self.expired() {
            return Err(out_of_time());
        }
        Ok(())
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub fn bounded(&self, wanted: Duration) -> Duration {
        wanted.min(self.remaining())
    }

    pub fn note_acting(&self) {
        self.acting.store(true, Ordering::SeqCst);
    }

    pub fn is_acting(&self) -> bool {
        self.acting.load(Ordering::SeqCst)
    }

    pub fn note_overlay_shown(&self) {
        self.overlay_shown.store(true, Ordering::SeqCst);
    }

    pub fn showed_overlay(&self) -> bool {
        self.overlay_shown.load(Ordering::SeqCst)
    }
}

pub fn working_deadline(started: Instant, allowed: Duration) -> Instant {
    started.checked_add(allowed.saturating_sub(SLACK)).unwrap_or(started)
}

pub fn out_of_time() -> Error {
    Error::timeout(OUT_OF_TIME)
}

#[cfg(test)]
impl CallScope {
    pub fn relaxed() -> Self {
        Self::new(Cancel::default(), working_deadline(Instant::now(), Duration::from_secs(600)))
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    #[test]
    fn the_working_deadline_leaves_a_second_of_slack() {
        let started = Instant::now();
        assert_eq!(working_deadline(started, Duration::from_secs(10)), started + Duration::from_secs(9));
        assert_eq!(working_deadline(started, Duration::from_millis(400)), started);
    }

    #[test]
    fn waits_are_bounded_by_the_time_left() {
        let scope = CallScope::new(Cancel::default(), Instant::now() + Duration::from_secs(2));
        assert_eq!(scope.bounded(Duration::from_millis(10)), Duration::from_millis(10));
        assert!(scope.bounded(Duration::from_secs(10)) <= Duration::from_secs(2));
        assert!(scope.check().is_ok() && !scope.expired());
    }

    #[test]
    fn a_passed_deadline_is_a_timeout_and_cancelling_wins() {
        let cancel = Cancel::default();
        let scope = CallScope::new(cancel.clone(), Instant::now());
        assert!(scope.expired());
        assert_eq!(scope.remaining(), Duration::ZERO);
        assert_eq!(scope.bounded(Duration::from_secs(5)), Duration::ZERO);
        assert_eq!(scope.check().unwrap_err().code, ErrorCode::Timeout);
        cancel.cancel();
        assert_eq!(scope.check().unwrap_err().code, ErrorCode::Cancelled);
    }

    #[test]
    fn the_flags_start_clear() {
        let scope = CallScope::relaxed();
        assert!(!scope.is_acting());
        scope.note_acting();
        assert!(scope.is_acting());
        assert!(!scope.showed_overlay());
        scope.note_overlay_shown();
        assert!(scope.showed_overlay());
    }
}
