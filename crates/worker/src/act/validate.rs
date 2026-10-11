use deckhand_core::{
    Error, Result, TargetSpec,
    keys::parse_keys,
    protocol::{ActArgs, Step},
};

use super::click_choice::parse_modifiers;
use crate::window_spec;

pub fn validate(args: &ActArgs) -> Result<()> {
    for (index, step) in args.steps.iter().enumerate() {
        check_step(step).map_err(|error| Error::new(error.code, format!("step {}: {}", index + 1, error.message)))?;
    }
    Ok(())
}

fn check_step(step: &Step) -> Result<()> {
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

    fn args(steps: &[&str]) -> ActArgs {
        let steps = steps.iter().map(|json| serde_json::from_str(json).unwrap()).collect();
        ActArgs { steps, after: AfterMode::None, stop_on_error: true, background: None }
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
}
