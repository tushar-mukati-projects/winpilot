use std::time::Duration;

use winpilot_core::{
    Error, Result, millis,
    protocol::{WaitArgs, WaitCondition},
};

use super::{done::Done, key_labels::chord_text};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    wait::{self, WaitReport, WindowsBefore},
};

const LONGEST_WAIT_WHILE_HELD: Duration = Duration::from_secs(1);

pub(crate) fn step(engine: &Engine, args: &WaitArgs, call: &CallScope, before: &WindowsBefore) -> Result<Done> {
    let condition = args.validate()?;
    refuse_long_hold(longest_wait(&condition, args.timeout()), &held_input(call))?;
    outcome(wait::until(engine, &condition, args.timeout(), before, call)?)
}

pub(crate) fn outcome(report: WaitReport) -> Result<Done> {
    if !report.satisfied {
        return Err(Error::timeout(format!("timed out after {} ms: {}", report.elapsed_ms, report.detail)));
    }
    if report.detail.is_empty() {
        return Ok(Done::default());
    }
    Ok(Done::noting(report.detail))
}

fn longest_wait(condition: &WaitCondition, timeout: Duration) -> Duration {
    match condition {
        WaitCondition::Time { wanted } => *wanted,
        _ => timeout,
    }
}

fn refuse_long_hold(longest: Duration, held: &[String]) -> Result<()> {
    if held.is_empty() || longest <= LONGEST_WAIT_WHILE_HELD {
        return Ok(());
    }
    Err(Error::invalid(format!(
        "this wait can last up to {} ms while {} {} held down, which would change the user's own keys and clicks; release first (key up, mouse up) or keep the wait at {} ms or less",
        millis(longest),
        held.join(" and "),
        if held.len() == 1 { "is" } else { "are" },
        millis(LONGEST_WAIT_WHILE_HELD)
    )))
}

fn held_input(call: &CallScope) -> Vec<String> {
    let mut held: Vec<String> = call.held_keys().iter().map(chord_text).collect();
    held.extend(call.held_buttons().into_iter().map(|button| format!("the {} mouse button", button.name())));
    held
}

#[cfg(test)]
mod tests {
    use winpilot_core::{ErrorCode, protocol::MouseButton};

    use super::*;

    fn report(satisfied: bool, detail: &str) -> WaitReport {
        WaitReport { satisfied, elapsed_ms: 250, detail: detail.to_string() }
    }

    fn condition(json: &str) -> WaitCondition {
        serde_json::from_str::<WaitArgs>(json).unwrap().validate().unwrap()
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

    #[test]
    fn a_wait_lasts_at_most_its_pause_or_its_timeout() {
        let timeout = Duration::from_millis(5000);
        assert_eq!(longest_wait(&condition(r#"{"ms":800}"#), timeout), Duration::from_millis(800));
        assert_eq!(longest_wait(&condition(r#"{"ms":5000}"#), timeout), Duration::from_secs(5));
        assert_eq!(longest_wait(&condition(r#"{"until":"idle"}"#), timeout), timeout);
        assert_eq!(longest_wait(&condition(r#"{"until":"window","window":"Save"}"#), timeout), timeout);
    }

    #[test]
    fn long_waits_are_refused_while_input_is_held() {
        let held = vec!["Shift".to_string(), "the left mouse button".to_string()];
        let error = refuse_long_hold(Duration::from_millis(5000), &held).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(
            error
                .message
                .starts_with("this wait can last up to 5000 ms while Shift and the left mouse button are held down"),
            "{}",
            error.message
        );
        assert!(error.message.ends_with("keep the wait at 1000 ms or less"), "{}", error.message);
        assert!(refuse_long_hold(Duration::from_millis(1000), &held).is_ok());
        assert!(refuse_long_hold(Duration::from_secs(60), &[]).is_ok());
    }

    #[test]
    fn held_input_is_named_for_the_refusal() {
        let call = CallScope::relaxed();
        call.note_held_keys(winpilot_core::keys::parse_chord("shift").unwrap());
        call.note_held(MouseButton::Left, winpilot_core::Point::new(1, 2));
        assert_eq!(held_input(&call), vec!["Shift".to_string(), "the left mouse button".to_string()]);
    }
}
