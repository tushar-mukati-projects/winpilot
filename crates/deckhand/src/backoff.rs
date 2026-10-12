use std::time::{Duration, Instant};

use deckhand_core::{Error, ErrorCode};

use crate::death::seconds;

const FIRST_RETRY: Duration = Duration::from_millis(500);
const LONGEST_RETRY: Duration = Duration::from_secs(10);
const MAX_DOUBLINGS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    cause: String,
    retry_at: Instant,
}

impl Failure {
    pub fn new(cause: String, failures: u32, now: Instant) -> Self {
        Self { cause, retry_at: now + delay(failures) }
    }

    pub fn is_over(&self, now: Instant) -> bool {
        now >= self.retry_at
    }

    pub fn error(&self, now: Instant) -> Error {
        let wait = round_up_to_tenth(self.retry_at.saturating_duration_since(now));
        let message = format!("the desktop worker failed to start: {}; retrying in {}", self.cause, seconds(wait));
        Error::new(ErrorCode::TargetNotResponding, message)
    }
}

fn delay(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(MAX_DOUBLINGS);
    FIRST_RETRY.saturating_mul(1 << doublings).min(LONGEST_RETRY)
}

fn round_up_to_tenth(duration: Duration) -> Duration {
    Duration::from_millis(duration.as_millis().div_ceil(100) as u64 * 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_double_from_half_a_second_up_to_ten() {
        let delays: Vec<Duration> = (1..=7).map(delay).collect();
        let millis: Vec<u128> = delays.iter().map(Duration::as_millis).collect();
        assert_eq!(millis, [500, 1000, 2000, 4000, 8000, 10_000, 10_000]);
        assert_eq!(delay(u32::MAX), LONGEST_RETRY);
    }

    #[test]
    fn failure_is_over_once_the_delay_passes() {
        let now = Instant::now();
        let failure = Failure::new("it exited".to_string(), 2, now);
        assert!(!failure.is_over(now + Duration::from_millis(999)));
        assert!(failure.is_over(now + Duration::from_secs(1)));
    }

    #[test]
    fn error_names_the_cause_and_the_time_left() {
        let now = Instant::now();
        let failure = Failure::new("it exited during startup".to_string(), 1, now);
        let error = failure.error(now);
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(error.message, "the desktop worker failed to start: it exited during startup; retrying in 0.5 s");
        let later = failure.error(now + Duration::from_millis(301));
        assert!(later.message.ends_with("retrying in 0.2 s"), "{}", later.message);
        let past = failure.error(now + Duration::from_secs(5));
        assert!(past.message.ends_with("retrying in 0 s"), "{}", past.message);
    }
}
