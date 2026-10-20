mod after;
mod append;
mod buttons;
mod by_pattern;
mod chords;
mod click;
mod click_choice;
mod clip;
mod clipboard;
mod done;
mod expand;
mod field;
mod focus;
mod focus_step;
mod foreground;
mod held;
mod hit;
mod key_labels;
mod keys;
mod landing;
mod launch;
mod launch_args;
mod launch_behind;
mod launch_match;
mod lift;
mod locate;
mod message;
mod message_keys;
mod mode;
mod motion;
mod mouse;
mod mouse_click;
mod mouse_select;
mod notify;
mod notify_limit;
mod option_search;
mod pacing;
mod paste;
mod pattern_click;
mod perform;
mod pick;
mod pointer;
mod refusal;
mod scroll;
mod select;
mod select_check;
mod set;
mod settle;
mod shortcut;
mod stacking;
mod steps;
mod summary;
mod target;
mod text_focus;
mod toggle;
mod typing;
mod validate;
mod value;
mod wait_step;
mod window;
mod window_behind;

#[cfg(test)]
mod input_live_tests;
#[cfg(test)]
mod live_tests;

use std::time::Instant;

use winpilot_core::{
    Error, Result, millis,
    protocol::{ActArgs, ActReport, After, AfterMode, StepOutcome},
};

pub(crate) use self::notify_limit::NotifyLimit;
use self::{after::Baseline, mode::Mode, settle::settle, steps::StepContext};
use crate::{call_scope::CallScope, engine::Engine, overlay, wait::WindowsBefore};

const AFTER_SKIPPED: &str = "after-state skipped: too little of the call's time was left to read the window";

pub(crate) fn run(engine: &Engine, args: &ActArgs, call: &CallScope) -> Result<ActReport> {
    let started = Instant::now();
    let report = run_holding_actor(engine, args, call, started);
    if call.showed_overlay() {
        overlay::end(engine);
    }
    report
}

fn run_holding_actor(engine: &Engine, args: &ActArgs, call: &CallScope, started: Instant) -> Result<ActReport> {
    args.validate()?;
    let mode = Mode::requested(args.background.unwrap_or(engine.config.act.background));
    validate::validate(args, &engine.guard, mode)?;
    if mode == Mode::Background {
        call.note_background();
    }
    let _actor = if steps::needs_actor(&args.steps) { Some(engine.guard.acquire_actor(call)?) } else { None };
    let before = WindowsBefore::now(engine);
    let baseline = Baseline::capture(engine, args.after);
    let mut outcomes = steps::run_all(engine, args, &StepContext { call, before: &before, mode, started });
    held::release_held(call, &mut outcomes);
    let after = after_or_note(engine, args.after, baseline.as_ref(), &mut outcomes, call)?;
    Ok(ActReport { steps: outcomes, total_steps: args.steps.len(), after, elapsed_ms: millis(started.elapsed()) })
}

fn after_or_note(
    engine: &Engine,
    mode: AfterMode,
    baseline: Option<&Baseline>,
    outcomes: &mut [StepOutcome],
    call: &CallScope,
) -> Result<After> {
    if mode != AfterMode::None && call.remaining() < engine.snapshot_budget() {
        return noted_instead(outcomes, AFTER_SKIPPED.to_string(), Error::timeout(AFTER_SKIPPED));
    }
    match after::report(engine, mode, baseline) {
        Ok(after) => Ok(after),
        Err(error) => noted_instead(outcomes, format!("after-state unavailable: {error}"), error),
    }
}

fn noted_instead(outcomes: &mut [StepOutcome], note: String, error: Error) -> Result<After> {
    let Some(last) = outcomes.last_mut() else { return Err(error) };
    steps::append_note(last, note);
    Ok(After::None)
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;
    use crate::test_fixture::{engine, step};

    fn batch(steps: Vec<winpilot_core::protocol::Step>) -> ActArgs {
        ActArgs { steps, after: AfterMode::None, stop_on_error: true, background: Some(false) }
    }

    #[test]
    fn an_empty_batch_reports_only_the_after_state() {
        let report = run(engine(), &batch(Vec::new()), &CallScope::relaxed()).unwrap();
        assert!(report.steps.is_empty());
        assert_eq!(report.total_steps, 0);
        assert_eq!(report.after, After::None);
    }

    #[test]
    fn too_many_steps_are_refused_before_anything_runs() {
        let pause = step(r#"{"do":"wait","ms":1}"#);
        let error = run(engine(), &batch(vec![pause; 51]), &CallScope::relaxed()).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
}
