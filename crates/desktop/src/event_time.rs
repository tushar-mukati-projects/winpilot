use std::time::{Duration, Instant};

use windows::Win32::System::SystemInformation::GetTickCount;

const TICK_GRANULARITY: Duration = Duration::from_millis(16);
const LATEST_PLAUSIBLE_AGE_MS: u32 = u32::MAX / 2;

pub(crate) fn win_event_instant(event_ms: u32) -> Instant {
    let now_ms = unsafe { GetTickCount() };
    event_instant(event_ms, now_ms, Instant::now())
}

fn event_instant(event_ms: u32, now_ms: u32, now: Instant) -> Instant {
    let age = event_age(event_ms, now_ms).saturating_sub(TICK_GRANULARITY);
    now.checked_sub(age).unwrap_or(now)
}

fn event_age(event_ms: u32, now_ms: u32) -> Duration {
    let elapsed = now_ms.wrapping_sub(event_ms);
    if elapsed > LATEST_PLAUSIBLE_AGE_MS {
        return Duration::ZERO;
    }
    Duration::from_millis(u64::from(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_event_is_dated_back_by_its_age_less_one_tick() {
        let now = Instant::now();
        assert_eq!(event_instant(1_000, 1_100, now), now.checked_sub(Duration::from_millis(84)).unwrap());
    }

    #[test]
    fn events_within_one_tick_count_as_now() {
        let now = Instant::now();
        assert_eq!(event_instant(5_000, 5_000, now), now);
        assert_eq!(event_instant(5_000, 5_016, now), now);
    }

    #[test]
    fn tick_count_wraparound_is_handled() {
        let now = Instant::now();
        assert_eq!(event_instant(u32::MAX - 49, 50, now), now.checked_sub(Duration::from_millis(84)).unwrap());
    }

    #[test]
    fn event_stamped_after_now_counts_as_now() {
        let now = Instant::now();
        assert_eq!(event_instant(2_000, 1_990, now), now);
    }

    #[test]
    fn event_queued_before_a_wait_starts_is_not_after_it() {
        let since = Instant::now();
        let delivered = since + Duration::from_millis(5);
        assert!(event_instant(10_000, 10_200, delivered) < since);
    }

    #[test]
    fn live_conversion_lands_near_now() {
        let before = Instant::now();
        let at = win_event_instant(unsafe { GetTickCount() });
        assert!(at >= before && at <= Instant::now());
    }
}
