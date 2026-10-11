use deckhand_core::{
    Error, Result, TargetSpec,
    keys::parse_keys,
    protocol::{ActArgs, Step},
};

use super::click_choice::parse_modifiers;

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
        Step::Hover { target, .. }
        | Step::Move { to: target, .. }
        | Step::MouseDown { target, .. }
        | Step::SetValue { target, .. }
        | Step::Select { target, .. }
        | Step::Toggle { target, .. }
        | Step::Expand { target }
        | Step::Collapse { target }
        | Step::Focus { target } => target.parse().map(drop),
        Step::Type { target, .. } | Step::Scroll { target, .. } | Step::MouseUp { target, .. } => {
            parse_optional(target.as_ref())
        }
        Step::Key { keys, target, .. } => {
            parse_keys(keys)?;
            parse_optional(target.as_ref())
        }
        Step::Drag { from, to, .. } => from.parse().and_then(|_| to.parse()).map(drop),
        Step::WaitFor(_) | Step::Wait { .. } => Ok(()),
    }
}

fn parse_optional(target: Option<&TargetSpec>) -> Result<()> {
    target.map(TargetSpec::parse).transpose().map(drop)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{
        ErrorCode,
        protocol::{AfterMode, ClickHow, MouseButton},
    };

    use super::*;

    fn args(steps: Vec<Step>) -> ActArgs {
        ActArgs { steps, after: AfterMode::None, stop_on_error: true }
    }

    fn text(target: &str) -> TargetSpec {
        TargetSpec::Text(target.to_string())
    }

    fn click_with(modifiers: Option<&str>) -> Step {
        Step::Click {
            target: text("e1"),
            button: MouseButton::Left,
            count: 1,
            modifiers: modifiers.map(str::to_string),
            how: ClickHow::Auto,
            glide_ms: None,
        }
    }

    #[test]
    fn empty_and_simple_batches_pass() {
        assert!(validate(&args(Vec::new())).is_ok());
        let steps = vec![
            Step::Key { keys: "ctrl+s".to_string(), target: None, repeat: 1 },
            Step::Wait { ms: 100 },
            Step::Drag { from: text("e1"), to: text("role:button name:OK"), glide_ms: None },
            click_with(Some("ctrl+shift")),
        ];
        assert!(validate(&args(steps)).is_ok());
    }

    #[test]
    fn a_bad_step_is_named_by_its_number() {
        let steps = vec![Step::Wait { ms: 1 }, Step::Key { keys: "ctrl+".to_string(), target: None, repeat: 1 }];
        let error = validate(&args(steps)).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.starts_with("step 2: "), "{}", error.message);
    }

    #[test]
    fn targets_keys_and_modifiers_are_checked() {
        let empty_target = Step::Focus { target: text("  ") };
        let bad_drag = Step::Drag { from: text("e1"), to: text(""), glide_ms: None };
        let bad_keys = Step::Key { keys: "ctrl+nope".to_string(), target: None, repeat: 1 };
        for step in [empty_target, click_with(Some("ctrl+x")), bad_drag, bad_keys] {
            assert!(validate(&args(vec![step.clone()])).is_err(), "{step:?}");
        }
    }
}
