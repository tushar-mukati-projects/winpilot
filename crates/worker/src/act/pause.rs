use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Result, protocol::MAX_PLAIN_WAIT};

use super::done::Done;
use crate::{call_scope::CallScope, engine::Engine};

const SLICE: Duration = Duration::from_millis(50);

pub fn step(engine: &Engine, ms: u64, call: &CallScope) -> Result<Done> {
    let (total, capped) = plain_wait(ms);
    let started = Instant::now();
    loop {
        engine.guard.check_running(call)?;
        let left = total.saturating_sub(started.elapsed());
        if left.is_zero() {
            break;
        }
        thread::sleep(call.bounded(left.min(SLICE)));
    }
    Ok(if capped { Done::noting("capped at 5 s") } else { Done::default() })
}

pub fn plain_wait(ms: u64) -> (Duration, bool) {
    let wanted = Duration::from_millis(ms);
    (wanted.min(MAX_PLAIN_WAIT), wanted > MAX_PLAIN_WAIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_waits_are_capped_at_five_seconds() {
        assert_eq!(plain_wait(0), (Duration::ZERO, false));
        assert_eq!(plain_wait(500), (Duration::from_millis(500), false));
        assert_eq!(plain_wait(5000), (MAX_PLAIN_WAIT, false));
        assert_eq!(plain_wait(5001), (MAX_PLAIN_WAIT, true));
        assert_eq!(plain_wait(u64::MAX), (MAX_PLAIN_WAIT, true));
    }
}
