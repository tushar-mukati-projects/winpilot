use winpilot_core::{Error, Result, TargetSpec, protocol::Step};

use super::{
    click,
    click_choice::ClickOptions,
    clipboard,
    done::Done,
    focus_step,
    keys::{self, KeyRequest},
    mode::{Mode, for_window},
    mouse::{self, MouseRequest},
    notify,
    pick::{self, PickRequest},
    refusal::background_refusal,
    set,
    steps::StepContext,
    target::{Prepared, prepare},
    typing::{self, TypeRequest},
    wait_step,
    window::{self, WindowRequest},
};
use crate::engine::Engine;

pub(crate) fn perform(engine: &Engine, step: &Step, context: &StepContext<'_>) -> Result<Done> {
    if context.mode == Mode::Background
        && let Some(refusal) = background_refusal(step)
    {
        return Err(refusal.in_background());
    }
    let prepared = primary_target(step).map(|spec| prepare(engine, spec, context.mode, context.call)).transpose()?;
    let mode = match &prepared {
        Some(prepared) => for_window(context.mode, &prepared.window, step)?,
        None => context.mode,
    };
    let note = primary_target(step).and_then(|spec| engine.images.lock().bare_point_note(spec));
    dispatch(engine, step, prepared.as_ref(), mode, context).map(|done| done.with_note(note))
}

fn primary_target(step: &Step) -> Option<&TargetSpec> {
    match step {
        Step::Click { target, .. } | Step::Set { target, .. } | Step::Focus { target } => Some(target),
        Step::Type { target, .. } | Step::Key { target, .. } | Step::Mouse { target, .. } => target.as_ref(),
        Step::Window { .. } | Step::Pick { .. } | Step::Wait(_) | Step::Clipboard { .. } | Step::Notify { .. } => None,
    }
}

fn dispatch(
    engine: &Engine,
    step: &Step,
    prepared: Option<&Prepared>,
    mode: Mode,
    context: &StepContext<'_>,
) -> Result<Done> {
    let call = context.call;
    match step {
        Step::Click { button, count, modifiers, how, glide_ms, .. } => {
            let options =
                ClickOptions { glide_ms: *glide_ms, ..ClickOptions::new(*button, *count, modifiers.as_deref(), *how)? };
            click::step(engine, required(prepared, step)?, options, mode, call)
        }
        Step::Type { text, enter, how, .. } => {
            typing::step(engine, prepared, &TypeRequest { text, enter: *enter, how: *how }, mode, call)
        }
        Step::Key { keys, repeat, action, .. } => {
            keys::step(engine, prepared, &KeyRequest { keys, repeat: *repeat, action: *action }, mode, call)
        }
        Step::Mouse { .. } => mouse::step(engine, &MouseRequest::try_from(step)?, prepared, step, mode, call),
        Step::Set { .. } => set::step(engine, required(prepared, step)?, &step.set_change()?, mode, call),
        Step::Focus { .. } => focus_step::step(engine, required(prepared, step)?, call),
        Step::Window { .. } => window::step(engine, &WindowRequest::try_from(step)?, step, context),
        Step::Pick { .. } => pick::step(engine, &PickRequest::try_from(step)?, step, mode, call),
        Step::Wait(args) => wait_step::step(engine, args, call, context.before),
        Step::Clipboard { text } => clipboard::step(engine, text, mode, call),
        Step::Notify { title, message } => notify::step(engine, title, message, context),
    }
}

pub(crate) fn required<'a>(prepared: Option<&'a Prepared>, step: &Step) -> Result<&'a Prepared> {
    prepared.ok_or_else(|| Error::invalid(format!("{} needs \"target\"", step.kind())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::step;

    #[test]
    fn input_steps_resolve_their_target_first_and_the_rest_do_not() {
        let targeted = [
            r#"{"do":"click","target":"e1"}"#,
            r#"{"do":"set","target":"e1","value":"x"}"#,
            r#"{"do":"focus","target":"e1"}"#,
            r#"{"do":"type","target":"e1","text":"x"}"#,
            r#"{"do":"key","keys":"a","target":"e1"}"#,
            r#"{"do":"mouse","action":"move","target":"e1"}"#,
        ];
        for json in targeted {
            assert_eq!(primary_target(&step(json)), Some(&TargetSpec::Text("e1".to_string())), "{json}");
        }
        let untargeted = [
            r#"{"do":"type","text":"x"}"#,
            r#"{"do":"mouse","action":"up"}"#,
            r#"{"do":"window","action":"close","target":"w1"}"#,
            r#"{"do":"pick","paths":["C:\\a.txt"],"open_with":"e1"}"#,
            r#"{"do":"wait","until":"appears","target":"e1"}"#,
        ];
        for json in untargeted {
            assert_eq!(primary_target(&step(json)), None, "{json}");
        }
    }

    #[test]
    fn a_missing_target_names_the_step() {
        let error = required(None, &step(r#"{"do":"focus","target":"e1"}"#)).unwrap_err();
        assert_eq!(error.message, "focus needs \"target\"");
    }
}
