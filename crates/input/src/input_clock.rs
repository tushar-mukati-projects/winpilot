use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const NEVER: u64 = 0;

static EPOCH: OnceLock<Instant> = OnceLock::new();
static LAST_USER_INPUT_MS: AtomicU64 = AtomicU64::new(NEVER);

pub(crate) fn start_clock() {
    EPOCH.get_or_init(Instant::now);
}

pub(crate) fn record_user_input() {
    let Some(epoch) = EPOCH.get() else {
        return;
    };
    LAST_USER_INPUT_MS.store(millis_since(*epoch, Instant::now()), Ordering::Relaxed);
}

pub(crate) fn last_user_input() -> Option<Instant> {
    let millis = LAST_USER_INPUT_MS.load(Ordering::Relaxed);
    if millis == NEVER {
        return None;
    }
    EPOCH.get()?.checked_add(Duration::from_millis(millis))
}

fn millis_since(epoch: Instant, now: Instant) -> u64 {
    let elapsed = now.saturating_duration_since(epoch).as_millis();
    u64::try_from(elapsed).unwrap_or(u64::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_are_never_zero_so_zero_can_mean_never() {
        let epoch = Instant::now();
        assert_eq!(millis_since(epoch, epoch), 1);
        assert_eq!(millis_since(epoch, epoch + Duration::from_millis(250)), 250);
    }

    #[test]
    fn time_before_the_epoch_saturates() {
        let now = Instant::now();
        assert_eq!(millis_since(now + Duration::from_secs(1), now), 1);
    }

    #[test]
    fn recorded_input_reads_back_as_an_instant() {
        start_clock();
        let before = Instant::now();
        record_user_input();
        let seen = last_user_input().unwrap();
        assert!(seen <= Instant::now() + Duration::from_millis(1));
        assert!(seen + Duration::from_millis(2) >= before);
    }
}
