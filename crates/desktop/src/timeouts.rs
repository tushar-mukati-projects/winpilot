use std::time::{Duration, Instant};

use windows::Win32::System::Threading::INFINITE;

const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365);

pub(crate) fn finite_millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX).min(INFINITE - 1)
}

pub(crate) fn later(start: Instant, duration: Duration) -> Instant {
    start.checked_add(duration).or_else(|| start.checked_add(FAR_FUTURE)).unwrap_or(start)
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
}
