use deckhand_core::{
    Error, Result,
    protocol::{Condition, WaitReport},
};

use super::done::Done;
use crate::{
    call_scope::CallScope,
    engine::Engine,
    wait::{self, WindowsBefore},
};

pub fn step(engine: &Engine, condition: &Condition, call: &CallScope, before: &WindowsBefore) -> Result<Done> {
    let parsed = condition.validate()?;
    let report = wait::until(engine, &parsed, condition.timeout(), before, call)?;
    outcome(report)
}

pub fn outcome(report: WaitReport) -> Result<Done> {
    if report.satisfied {
        return Ok(Done::noting(report.detail));
    }
    Err(Error::timeout(format!("timed out after {} ms: {}", report.elapsed_ms, report.detail)))
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    fn report(satisfied: bool) -> WaitReport {
        WaitReport { satisfied, elapsed_ms: 250, detail: "id:2016 is visible".to_string(), element: None }
    }

    #[test]
    fn a_satisfied_wait_notes_its_detail() {
        assert_eq!(outcome(report(true)).unwrap(), Done::noting("id:2016 is visible"));
    }

    #[test]
    fn an_unsatisfied_wait_is_a_timeout() {
        let error = outcome(report(false)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert_eq!(error.message, "timed out after 250 ms: id:2016 is visible");
    }
}
