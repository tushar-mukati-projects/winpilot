use std::time::{Duration, Instant};

use deckhand_core::{Error, ErrorCode, Result};

pub const MAX_PER_CALL: usize = 3;
const GAP_BETWEEN_CALLS: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct NotifyLimit {
    last_sent: Option<Instant>,
}

impl NotifyLimit {
    pub fn admit(&mut self, call_started: Instant, now: Instant) -> Result<()> {
        if let Some(left) = self.wait_left(call_started, now) {
            return Err(too_soon(left));
        }
        self.last_sent = Some(now);
        Ok(())
    }

    fn wait_left(&self, call_started: Instant, now: Instant) -> Option<Duration> {
        let earlier_call = self.last_sent.filter(|sent| *sent < call_started)?;
        let left = GAP_BETWEEN_CALLS.saturating_sub(now.saturating_duration_since(earlier_call));
        (!left.is_zero()).then_some(left)
    }
}

pub fn too_many(count: usize) -> Error {
    Error::invalid(format!(
        "a call may send at most {MAX_PER_CALL} notifications so the user isn't flooded, and this one has {count}; put the news in fewer notify steps"
    ))
}

fn too_soon(left: Duration) -> Error {
    Error::new(
        ErrorCode::Busy,
        format!(
            "notifications from separate calls must be at least {} s apart so the user isn't flooded; this one was not shown, so send it in {:.1} s or fold it into the next notification",
            GAP_BETWEEN_CALLS.as_secs(),
            left.as_secs_f64()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn a_call_may_notify_several_times_in_a_row() {
        let started = Instant::now();
        let mut limit = NotifyLimit::default();
        assert!(limit.admit(started, started).is_ok());
        assert!(limit.admit(started, started + MS(10)).is_ok());
        assert!(limit.admit(started, started + MS(20)).is_ok());
    }

    #[test]
    fn the_next_call_waits_five_seconds_after_the_last_notification() {
        let first = Instant::now();
        let mut limit = NotifyLimit::default();
        limit.admit(first, first).unwrap();
        let next_call = first + MS(1000);
        let error = limit.admit(next_call, next_call + MS(500)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        assert!(error.message.contains("at least 5 s apart"), "{}", error.message);
        assert!(error.message.contains("send it in 3.5 s"), "{}", error.message);
        let later_call = first + MS(5000);
        assert!(limit.admit(later_call, later_call).is_ok());
    }

    #[test]
    fn a_refused_notification_does_not_restart_the_clock() {
        let first = Instant::now();
        let mut limit = NotifyLimit::default();
        limit.admit(first, first).unwrap();
        let soon = first + MS(4000);
        assert!(limit.admit(soon, soon).is_err());
        let after = first + MS(5001);
        assert!(limit.admit(after, after).is_ok());
    }

    #[test]
    fn too_many_notify_steps_are_named() {
        let error = too_many(5);
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.starts_with("a call may send at most 3 notifications"), "{}", error.message);
        assert!(error.message.contains("this one has 5"), "{}", error.message);
    }
}
