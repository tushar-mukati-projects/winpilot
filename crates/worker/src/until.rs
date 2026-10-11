use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::Result;

use crate::{call_scope::CallScope, engine::Engine, watch::Watch};

const RECHECK: Duration = Duration::from_millis(250);
const MIN_INTERVAL: Duration = Duration::from_millis(50);

pub fn until<T>(
    engine: &Engine,
    call: &CallScope,
    limit: Duration,
    watch: &Watch,
    mut probe: impl FnMut() -> Result<Option<T>>,
) -> Result<Option<T>> {
    let started = Instant::now();
    loop {
        let checked = Instant::now();
        if let Some(found) = probe()? {
            return Ok(Some(found));
        }
        let left = limit.saturating_sub(started.elapsed());
        if left.is_zero() {
            return Ok(None);
        }
        engine.guard.check_waiting(call)?;
        watch.wait_for_change(engine, checked, next_slice(left, call.remaining()));
        let left = limit.saturating_sub(started.elapsed()).min(call.remaining());
        thread::sleep(rest_before_recheck(checked.elapsed(), left));
    }
}

pub fn next_slice(left: Duration, call_remaining: Duration) -> Duration {
    left.min(call_remaining).min(RECHECK)
}

pub fn rest_before_recheck(since_check: Duration, left: Duration) -> Duration {
    MIN_INTERVAL.saturating_sub(since_check).min(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn slices_never_outlast_the_wait_or_the_call() {
        assert_eq!(next_slice(MS(5000), MS(9000)), RECHECK);
        assert_eq!(next_slice(MS(100), MS(9000)), MS(100));
        assert_eq!(next_slice(MS(5000), MS(30)), MS(30));
        assert_eq!(next_slice(MS(5000), Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn rechecks_are_spaced_at_least_fifty_milliseconds_apart() {
        assert_eq!(rest_before_recheck(MS(0), MS(1000)), MIN_INTERVAL);
        assert_eq!(rest_before_recheck(MS(20), MS(1000)), MS(30));
        assert_eq!(rest_before_recheck(MS(80), MS(1000)), Duration::ZERO);
        assert_eq!(rest_before_recheck(MS(10), MS(15)), MS(15));
    }
}
