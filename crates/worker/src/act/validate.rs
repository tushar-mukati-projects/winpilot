use deckhand_core::{
    Error, Result, TargetSpec,
    keys::parse_keys,
    protocol::{ActArgs, Step},
};

use super::{
    click_choice::parse_modifiers,
    mode::Mode,
    notify_limit::{MAX_PER_CALL, too_many},
    pick::check_paths,
};
use crate::{guard::Guard, window_spec};

pub(crate) fn validate(args: &ActArgs, guard: &Guard, mode: Mode) -> Result<()> {
    for (index, step) in args.steps.iter().enumerate() {
        check_step(step, guard, mode)
            .map_err(|error| Error::new(error.code, format!("step {}: {}", index + 1, error.message)))?;
    }
    check_notifications(&args.steps)
}

fn check_notifications(steps: &[Step]) -> Result<()> {
    let count = steps.iter().filter(|step| matches!(step, Step::Notify { .. })).count();
    if count > MAX_PER_CALL {
        return Err(too_many(count));
    }
    Ok(())
}

fn check_step(step: &Step, guard: &Guard, mode: Mode) -> Result<()> {
    match step {
        Step::Type { target: None, .. } if mode == Mode::Background => {
            Err(Error::invalid("type in background mode needs a target"))
        }
        Step::Pick { paths, .. } => check_paths(paths, guard).and_then(|()| check_targets(step)),
        Step::Wait(wait) => wait.window.as_deref().map(window_spec::parse).transpose().map(drop),
        _ => check_targets(step),
    }
}

fn check_targets(step: &Step) -> Result<()> {
    match step {
        Step::Click { target, modifiers, .. } => {
            target.parse()?;
            modifiers.as_deref().map(parse_modifiers).transpose().map(drop)
        }
        Step::Set { target, .. } | Step::Focus { target } => target.parse().map(drop),
        Step::Type { target, .. } => parse_optional(target.as_ref()),
        Step::Window { target, .. } => target.as_ref().map_or(Ok(()), parse_window),
        Step::Mouse { target, to, .. } => parse_optional(target.as_ref()).and_then(|()| parse_optional(to.as_ref())),
        Step::Key { keys, target, .. } => {
            parse_keys(keys)?;
            parse_optional(target.as_ref())
        }
        Step::Pick { open_with, .. } => parse_optional(open_with.as_ref()),
        Step::Wait(_) | Step::Clipboard { .. } | Step::Notify { .. } => Ok(()),
    }
}

fn parse_window(target: &TargetSpec) -> Result<()> {
    match target {
        TargetSpec::Text(text) => window_spec::parse(text).map(drop),
        TargetSpec::Point(_) => target.parse().map(drop),
    }
}

fn parse_optional(target: Option<&TargetSpec>) -> Result<()> {
    target.map(TargetSpec::parse).transpose().map(drop)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, protocol::AfterMode};

    use super::*;
    use crate::test_fixture::engine;

    fn args(steps: &[&str]) -> ActArgs {
        let steps = steps.iter().map(|json| serde_json::from_str(json).unwrap()).collect();
        ActArgs { steps, after: AfterMode::None, stop_on_error: true, background: None }
    }

    fn validate_in(args: &ActArgs, mode: Mode) -> Result<()> {
        super::validate(args, &engine().guard, mode)
    }

    fn validate(args: &ActArgs) -> Result<()> {
        validate_in(args, Mode::Foreground)
    }

    #[test]
    fn empty_and_simple_batches_pass() {
        assert!(validate(&args(&[])).is_ok());
        let steps = [
            r#"{"do":"key","keys":"ctrl+s"}"#,
            r#"{"do":"wait","ms":100}"#,
            r#"{"do":"mouse","action":"drag","target":"e1","to":"role:button name:OK"}"#,
            r#"{"do":"click","target":"e1","modifiers":"ctrl+shift"}"#,
            r#"{"do":"window","action":"close","target":"process:notepad"}"#,
            r#"{"do":"window","action":"close","target":"C:\\a.txt - Notepad"}"#,
        ];
        assert!(validate(&args(&steps)).is_ok());
    }

    #[test]
    fn a_bad_step_is_named_by_its_number() {
        let error = validate(&args(&[r#"{"do":"wait","ms":1}"#, r#"{"do":"key","keys":"ctrl+"}"#])).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.starts_with("step 2: "), "{}", error.message);
    }

    #[test]
    fn targets_keys_and_modifiers_are_checked() {
        for json in [
            r#"{"do":"focus","target":"  "}"#,
            r#"{"do":"click","target":"e1","modifiers":"ctrl+x"}"#,
            r#"{"do":"mouse","action":"drag","target":"e1","to":""}"#,
            r#"{"do":"key","keys":"ctrl+nope"}"#,
            r#"{"do":"pick","paths":["C:\\a.txt"],"open_with":"role:gizmo"}"#,
        ] {
            assert!(validate(&args(&[json])).is_err(), "{json}");
        }
    }

    #[test]
    fn later_steps_that_cannot_run_are_caught_before_anything_runs() {
        let relative = validate(&args(&[r#"{"do":"wait","ms":1}"#, r#"{"do":"pick","paths":["notes.txt"]}"#]));
        assert!(relative.unwrap_err().message.starts_with("step 2: "));
        let missing_folder = r#"{"do":"pick","paths":["C:\\deckhand-no-such-folder\\a.txt"]}"#;
        assert_eq!(validate(&args(&[missing_folder])).unwrap_err().code, ErrorCode::NotFound);
        let bad_scope = r#"{"do":"wait","until":"text","text":"Done","window":"role:button"}"#;
        assert!(validate(&args(&[bad_scope])).is_err());
        let untargeted = args(&[r#"{"do":"type","text":"hi"}"#]);
        assert!(validate(&untargeted).is_ok());
        let refused = validate_in(&untargeted, Mode::Background).unwrap_err();
        assert_eq!(refused.message, "step 1: type in background mode needs a target");
    }

    #[test]
    fn a_call_may_hold_at_most_three_notify_steps() {
        let notify = r#"{"do":"notify","title":"a","message":"b"}"#;
        assert!(validate(&args(&[notify, notify, notify])).is_ok());
        let error = validate(&args(&[notify, notify, r#"{"do":"wait","ms":1}"#, notify, notify])).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.contains("this one has 4"), "{}", error.message);
    }
}
