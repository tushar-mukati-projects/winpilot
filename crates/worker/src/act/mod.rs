mod after;
mod chords;
mod click;
mod click_choice;
mod clip;
mod done;
mod expand;
mod focus;
mod focus_step;
mod hit;
mod keys;
mod landing;
mod locate;
mod mouse_click;
mod mouse_select;
mod option_search;
mod overlay;
mod paste;
mod pattern_click;
mod pause;
mod perform;
mod pointer;
mod scroll;
mod select;
mod select_check;
mod steps;
mod summary;
mod target;
mod toggle;
mod typing;
mod validate;
mod value;
mod wait_step;
mod window;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod input_tests;

use std::time::{Duration, Instant};

use deckhand_core::{
    Error, Result, TargetSpec, WindowInfo,
    protocol::{ActArgs, ActReport, After, AfterMode, StepOutcome},
};

use self::{after::Baseline, click_choice::ClickOptions};
use crate::{
    call_scope::CallScope,
    engine::{Engine, duration_ms},
    wait::WindowsBefore,
    watch::Watch,
};

const RESPONSIVE_WAIT: Duration = Duration::from_millis(500);
const AFTER_SKIPPED: &str = "after-state skipped: the call ran out of time";

pub fn run(engine: &Engine, args: &ActArgs, call: &CallScope) -> Result<ActReport> {
    let started = Instant::now();
    let report = run_holding_actor(engine, args, call, started);
    if call.showed_overlay() {
        overlay::hide(engine);
    }
    report
}

pub fn click_target(engine: &Engine, target: &TargetSpec, call: &CallScope) -> Result<WindowInfo> {
    let clicked = target::prepare(engine, target, call).and_then(|prepared| {
        click::click_prepared(engine, &prepared, ClickOptions::default(), call)?;
        settle(engine, &prepared.window, call);
        Ok(prepared.window)
    });
    if call.showed_overlay() {
        overlay::hide(engine);
    }
    clicked
}

pub fn settle(engine: &Engine, window: &WindowInfo, call: &CallScope) {
    let _responsive = deckhand_desktop::wait_until_responsive(window.hwnd, call.bounded(RESPONSIVE_WAIT));
    let quiet = Duration::from_millis(engine.config.act.settle_quiet_ms);
    let max = call.bounded(Duration::from_millis(engine.config.act.settle_max_ms));
    let _quiet = Watch::window(window).wait_quiet(engine, quiet, max);
}

fn run_holding_actor(engine: &Engine, args: &ActArgs, call: &CallScope, started: Instant) -> Result<ActReport> {
    args.validate()?;
    validate::validate(args)?;
    let _actor = if steps::needs_actor(&args.steps) { Some(engine.guard.acquire_actor(call)?) } else { None };
    let before = WindowsBefore::now(engine);
    let baseline = Baseline::capture(engine, args.after);
    let mut outcomes = steps::run_all(engine, args, call, &before);
    let after = after_or_note(engine, args.after, baseline.as_ref(), &mut outcomes, call)?;
    Ok(ActReport { steps: outcomes, total_steps: args.steps.len(), after, elapsed_ms: duration_ms(started.elapsed()) })
}

fn after_or_note(
    engine: &Engine,
    mode: AfterMode,
    baseline: Option<&Baseline>,
    outcomes: &mut [StepOutcome],
    call: &CallScope,
) -> Result<After> {
    if mode != AfterMode::None && call.expired() {
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
