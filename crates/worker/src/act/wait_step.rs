use deckhand_core::{Error, Result, protocol::WaitArgs};

use super::done::Done;
use crate::{
    call_scope::CallScope,
    engine::Engine,
    wait::{self, WaitReport, WindowsBefore},
};

pub fn step(engine: &Engine, args: &WaitArgs, call: &CallScope, before: &WindowsBefore) -> Result<Done> {
    let condition = args.validate()?;
    outcome(wait::until(engine, &condition, args.timeout(), before, call)?)
}

pub fn outcome(report: WaitReport) -> Result<Done> {
    if !report.satisfied {
        return Err(Error::timeout(format!("timed out after {} ms: {}", report.elapsed_ms, report.detail)));
    }
    if report.detail.is_empty() {
        return Ok(Done::default());
    }
    Ok(Done::noting(report.detail))
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    fn report(satisfied: bool, detail: &str) -> WaitReport {
        WaitReport { satisfied, elapsed_ms: 250, detail: detail.to_string(), element: None }
    }

    #[test]
    fn a_satisfied_wait_notes_its_detail() {
        assert_eq!(outcome(report(true, "id:2016 is visible")).unwrap(), Done::noting("id:2016 is visible"));
        assert_eq!(outcome(report(true, "")).unwrap(), Done::default());
    }

    #[test]
    fn an_unsatisfied_wait_is_a_timeout() {
        let error = outcome(report(false, "id:2016 is visible")).unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert_eq!(error.message, "timed out after 250 ms: id:2016 is visible");
    }
}
