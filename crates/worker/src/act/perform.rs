use deckhand_core::{Error, Result, TargetSpec, protocol::Step};

use super::{
    click::{self},
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

pub fn perform(engine: &Engine, step: &Step, context: &StepContext<'_>) -> Result<Done> {
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
    dispatch(engine, step, prepared.as_ref(), mode, context)
}

pub fn primary_target(step: &Step) -> Option<&TargetSpec> {
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
            keys::step(engine, prepared, &KeyRequest { keys, repeat: *repeat, action: *action }, call)
        }
        Step::Mouse { action, to, button, direction, amount, glide_ms, .. } => {
            let request = MouseRequest {
                action: *action,
                to: to.as_ref(),
                button: *button,
                direction: *direction,
                amount: *amount,
                glide_ms: *glide_ms,
            };
            mouse::step(engine, &request, prepared, step, mode, call)
        }
        Step::Set { .. } => set::step(engine, required(prepared, step)?, &step.set_change()?, mode, call),
        Step::Focus { .. } => focus_step::step(engine, required(prepared, step)?, call),
        Step::Window { action, target, app, args, rect } => {
            let request = WindowRequest {
                action: *action,
                target: target.as_ref(),
                app: app.as_deref(),
                args: args.as_deref(),
                rect: *rect,
            };
            window::step(engine, &request, step, context)
        }
        Step::Pick { paths, open_with, overwrite, timeout_ms } => {
            let request =
                PickRequest { paths, open_with: open_with.as_ref(), overwrite: *overwrite, timeout_ms: *timeout_ms };
            pick::step(engine, &request, step, call)
        }
        Step::Wait(args) => wait_step::step(engine, args, call, context.before),
        Step::Clipboard { text } => clipboard::step(engine, text, mode, call),
        Step::Notify { title, message } => notify::step(engine, title, message, call),
    }
}

pub fn required<'a>(prepared: Option<&'a Prepared>, step: &Step) -> Result<&'a Prepared> {
    prepared.ok_or_else(|| Error::invalid(format!("{} needs \"target\"", step.kind())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

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
