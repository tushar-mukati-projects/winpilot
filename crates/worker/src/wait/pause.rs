use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Result, protocol::plain_wait};

use super::WaitReport;
use crate::{
    call_scope::CallScope,
    engine::{Engine, duration_ms},
};

const SLICE: Duration = Duration::from_millis(50);
const CAPPED: &str = "capped at 5 s";

pub fn pause(engine: &Engine, wanted: Duration, call: &CallScope) -> Result<WaitReport> {
    let (total, capped) = plain_wait(wanted);
    let started = Instant::now();
    loop {
        engine.guard.check_running(call)?;
        let left = total.saturating_sub(started.elapsed());
        if left.is_zero() {
            break;
        }
        thread::sleep(call.bounded(left.min(SLICE)));
    }
    let detail = if capped { CAPPED.to_string() } else { String::new() };
    Ok(WaitReport { satisfied: true, elapsed_ms: duration_ms(started.elapsed()), detail, element: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn a_short_pause_is_satisfied_without_a_note() {
        let report = pause(engine(), Duration::from_millis(5), &CallScope::relaxed()).unwrap();
        assert!(report.satisfied && report.detail.is_empty());
    }
}
