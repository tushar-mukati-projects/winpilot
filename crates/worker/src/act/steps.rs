use std::time::Instant;

use winpilot_core::{
    Error, ErrorCode, Result,
    protocol::{ActArgs, Step, StepOutcome},
};

use super::{
    done::{Done, joined},
    mode::Mode,
    perform::perform,
    summary::summarize,
};
use crate::{call_scope::CallScope, engine::Engine, wait::WindowsBefore};

pub(crate) struct StepContext<'a> {
    pub(crate) call: &'a CallScope,
    pub(crate) before: &'a WindowsBefore,
    pub(crate) mode: Mode,
    pub(crate) started: Instant,
}

pub(crate) fn run_all(engine: &Engine, args: &ActArgs, context: &StepContext<'_>) -> Vec<StepOutcome> {
    let mut outcomes = Vec::with_capacity(args.steps.len());
    for (index, step) in args.steps.iter().enumerate() {
        let result = engine.guard.check_running(context.call).and_then(|()| perform(engine, step, context));
        if step.uses_input() {
            context.call.forget_ocr();
        }
        let outcome = outcome(index, summarize(step), result);
        let stop =
            outcome.error.as_ref().is_some_and(|error| args.stop_on_error || halts(error) || context.call.expired());
        outcomes.push(outcome);
        if stop {
            break;
        }
    }
    outcomes
}

pub(crate) fn outcome(index: usize, step: String, result: Result<Done>) -> StepOutcome {
    match result {
        Ok(done) => StepOutcome { index, step, method: done.method, note: done.note, error: None },
        Err(error) => StepOutcome { index, step, method: None, note: None, error: Some(error) },
    }
}

fn halts(error: &Error) -> bool {
    matches!(error.code, ErrorCode::StoppedByUser | ErrorCode::Cancelled)
}

pub(crate) fn needs_actor(steps: &[Step]) -> bool {
    steps.iter().any(Step::uses_input)
}

pub(crate) fn append_note(outcome: &mut StepOutcome, note: String) {
    outcome.note = Some(joined(outcome.note.take(), note));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::step;

    #[test]
    fn successes_carry_method_and_note() {
        let done = Done::by("invoke").with_note(Some("value now \"x\"".to_string()));
        let outcome = outcome(2, "click e12".to_string(), Ok(done));
        assert_eq!(outcome.index, 2);
        assert_eq!(outcome.step, "click e12");
        assert_eq!(outcome.method.as_deref(), Some("invoke"));
        assert_eq!(outcome.note.as_deref(), Some("value now \"x\""));
        assert!(outcome.error.is_none());
    }

    #[test]
    fn failures_carry_only_the_error() {
        let outcome = outcome(0, "click e12".to_string(), Err(Error::not_found("gone")));
        assert_eq!(outcome.error.map(|error| error.code), Some(ErrorCode::NotFound));
        assert!(outcome.method.is_none() && outcome.note.is_none());
    }

    #[test]
    fn stops_and_cancels_halt_every_batch() {
        assert!(halts(&Error::new(ErrorCode::StoppedByUser, "stopped")));
        assert!(halts(&Error::cancelled("cancelled")));
        assert!(!halts(&Error::not_found("gone")));
    }

    #[test]
    fn only_waits_and_notifications_skip_the_actor_lock() {
        assert!(!needs_actor(&[]));
        let quiet = [step(r#"{"do":"wait","ms":10}"#), step(r#"{"do":"notify","title":"a","message":"b"}"#)];
        assert!(!needs_actor(&quiet));
        assert!(needs_actor(&[step(r#"{"do":"wait","ms":10}"#), step(r#"{"do":"focus","target":"e1"}"#)]));
        assert!(needs_actor(&[step(r#"{"do":"clipboard","text":"x"}"#)]));
    }

    #[test]
    fn notes_are_appended() {
        let mut outcome = outcome(0, "type \"x\"".to_string(), Ok(Done::by("keys")));
        append_note(&mut outcome, "after-state unavailable".to_string());
        assert_eq!(outcome.note.as_deref(), Some("after-state unavailable"));
        append_note(&mut outcome, "again".to_string());
        assert_eq!(outcome.note.as_deref(), Some("after-state unavailable; again"));
    }
}
