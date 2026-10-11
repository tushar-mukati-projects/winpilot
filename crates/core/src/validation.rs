use std::{fmt::Display, ops::RangeInclusive};

use crate::{
    error::{Error, Result},
    geometry::{COORD_LIMIT, Rect},
    protocol::{ActArgs, ReadTextArgs, ScreenshotArgs, Step, WindowsArgs},
};

const MAX_STEPS: usize = 50;
const MAX_TEXT_CHARS: usize = 100_000;
const MAX_WAIT_STEP_MS: u64 = 60_000;
const CLICK_COUNTS: RangeInclusive<u8> = 1..=3;
const KEY_REPEATS: RangeInclusive<u32> = 1..=100;
const SCROLL_AMOUNTS: RangeInclusive<u32> = 1..=50;

impl ActArgs {
    pub fn validate(&self) -> Result<()> {
        if self.steps.len() > MAX_STEPS {
            return Err(Error::invalid(format!("act takes at most {MAX_STEPS} steps, got {}", self.steps.len())));
        }
        for (index, step) in self.steps.iter().enumerate() {
            step.validate()
                .map_err(|error| Error::new(error.code, format!("step {}: {}", index + 1, error.message)))?;
        }
        Ok(())
    }
}

impl Step {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Click { count, .. } => check_range("click count", *count, &CLICK_COUNTS),
            Self::Type { text, .. } => check_text_length("text", text),
            Self::SetValue { value, .. } => check_text_length("value", value),
            Self::Key { repeat, .. } => check_range("key repeat", *repeat, &KEY_REPEATS),
            Self::Scroll { amount, .. } => check_range("scroll amount", *amount, &SCROLL_AMOUNTS),
            Self::WaitFor(condition) => condition.validate().map(drop),
            Self::Wait { ms } => check_wait(*ms),
            Self::Hover { .. }
            | Self::Drag { .. }
            | Self::Select { .. }
            | Self::Toggle { .. }
            | Self::Expand { .. }
            | Self::Collapse { .. }
            | Self::Focus { .. } => Ok(()),
        }
    }
}

fn check_range<T: PartialOrd + Display + Copy>(what: &str, value: T, allowed: &RangeInclusive<T>) -> Result<()> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(Error::invalid(format!("{what} must be from {} to {}, got {value}", allowed.start(), allowed.end())))
}

fn check_text_length(what: &str, text: &str) -> Result<()> {
    if text.chars().nth(MAX_TEXT_CHARS).is_none() {
        return Ok(());
    }
    Err(Error::invalid(format!("{what} is longer than {MAX_TEXT_CHARS} characters; split it across steps")))
}

fn check_wait(ms: u64) -> Result<()> {
    if ms <= MAX_WAIT_STEP_MS {
        return Ok(());
    }
    Err(Error::invalid(format!(
        "wait ms must be at most {MAX_WAIT_STEP_MS}, got {ms}; use wait_for to wait for a change"
    )))
}

fn check_screen_rect(field: &str, rect: Option<Rect>) -> Result<()> {
    let Some(rect) = rect else { return Ok(()) };
    if !rect.is_empty() && rect.is_plausible() {
        return Ok(());
    }
    let Rect { x, y, width, height } = rect;
    Err(Error::invalid(format!(
        "{field} {x},{y} {width}x{height} needs a positive size and must stay within ±{COORD_LIMIT} pixels"
    )))
}

impl ScreenshotArgs {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("region", self.region)
    }
}

impl ReadTextArgs {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("region", self.region)
    }
}

impl WindowsArgs {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("rect", self.rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::ErrorCode,
        protocol::{AfterMode, TypeHow, WindowAction},
        target::TargetSpec,
    };

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn invalid_message(result: Result<()>) -> String {
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    #[test]
    fn step_parameters_are_range_checked() {
        let cases = [
            (r#"{"do":"click","target":"e1","count":0}"#, "click count must be from 1 to 3, got 0"),
            (r#"{"do":"click","target":"e1","count":4}"#, "click count must be from 1 to 3, got 4"),
            (r#"{"do":"key","keys":"tab","repeat":0}"#, "key repeat must be from 1 to 100, got 0"),
            (r#"{"do":"key","keys":"tab","repeat":101}"#, "key repeat must be from 1 to 100, got 101"),
            (r#"{"do":"scroll","direction":"down","amount":0}"#, "scroll amount must be from 1 to 50, got 0"),
            (r#"{"do":"scroll","direction":"down","amount":51}"#, "scroll amount must be from 1 to 50, got 51"),
            (
                r#"{"do":"wait","ms":60001}"#,
                "wait ms must be at most 60000, got 60001; use wait_for to wait for a change",
            ),
            (r#"{"do":"wait_for","until":"text"}"#, "until=text needs \"text\""),
        ];
        for (json, expected) in cases {
            assert_eq!(invalid_message(step(json).validate()), expected, "{json}");
        }
        for json in [
            r#"{"do":"click","target":"e1","count":3}"#,
            r#"{"do":"key","keys":"tab","repeat":100}"#,
            r#"{"do":"scroll","direction":"up","amount":50}"#,
            r#"{"do":"wait","ms":60000}"#,
            r#"{"do":"hover","target":"e1"}"#,
        ] {
            assert!(step(json).validate().is_ok(), "{json}");
        }
    }

    #[test]
    fn typed_text_length_is_capped() {
        let at_limit = Step::Type { target: None, text: "é".repeat(MAX_TEXT_CHARS), enter: false, how: TypeHow::Auto };
        assert!(at_limit.validate().is_ok());
        let over = Step::Type { target: None, text: "a".repeat(MAX_TEXT_CHARS + 1), enter: false, how: TypeHow::Auto };
        assert_eq!(invalid_message(over.validate()), "text is longer than 100000 characters; split it across steps");
        let value =
            Step::SetValue { target: TargetSpec::Text("e1".to_string()), value: "v".repeat(MAX_TEXT_CHARS + 1) };
        assert!(invalid_message(value.validate()).starts_with("value is longer"));
    }

    #[test]
    fn act_validation_checks_step_count_and_names_the_failing_step() {
        let wait = Step::Wait { ms: 1 };
        let at_limit = ActArgs { steps: vec![wait.clone(); MAX_STEPS], after: AfterMode::Diff, stop_on_error: true };
        assert!(at_limit.validate().is_ok());
        let over = ActArgs { steps: vec![wait.clone(); MAX_STEPS + 1], ..at_limit.clone() };
        assert_eq!(invalid_message(over.validate()), "act takes at most 50 steps, got 51");
        let bad = ActArgs { steps: vec![wait, step(r#"{"do":"click","target":"e1","count":9}"#)], ..at_limit };
        assert_eq!(invalid_message(bad.validate()), "step 2: click count must be from 1 to 3, got 9");
        let empty = ActArgs { steps: Vec::new(), after: AfterMode::None, stop_on_error: true };
        assert!(empty.validate().is_ok());
    }

    #[test]
    fn screen_rectangles_are_validated() {
        let fine = Some(Rect::new(-1920, 0, 800, 600));
        assert!(ScreenshotArgs { region: fine, ..ScreenshotArgs::default() }.validate().is_ok());
        assert!(ScreenshotArgs::default().validate().is_ok());
        let far = ScreenshotArgs { region: Some(Rect::new(i32::MAX - 5, 0, 100, 100)), ..ScreenshotArgs::default() };
        assert_eq!(
            invalid_message(far.validate()),
            format!("region {},0 100x100 needs a positive size and must stay within ±100000 pixels", i32::MAX - 5)
        );
        let empty = ReadTextArgs { region: Some(Rect::new(0, 0, 0, 10)), ..ReadTextArgs::default() };
        assert!(empty.validate().is_err());
        assert!(ReadTextArgs { region: fine, ..ReadTextArgs::default() }.validate().is_ok());
        let negative = WindowsArgs {
            action: WindowAction::Move,
            window: None,
            app: None,
            args: None,
            rect: Some(Rect::new(0, 0, -5, 10)),
        };
        assert_eq!(
            invalid_message(negative.validate()),
            "rect 0,0 -5x10 needs a positive size and must stay within ±100000 pixels"
        );
        assert!(WindowsArgs { rect: fine, ..negative }.validate().is_ok());
    }
}
