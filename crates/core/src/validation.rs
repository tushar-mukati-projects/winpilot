mod requests;

use std::{fmt::Display, ops::RangeInclusive};

use crate::{
    error::{Error, Result},
    field_rules::{does_not_apply, first_stray},
    geometry::{COORD_LIMIT, Rect},
    glide_timing::MAX_GLIDE_MS,
    keys::parse_keys,
    protocol::{NOTIFY_MESSAGE_MAX, NOTIFY_TITLE_MAX},
    steps::{ActArgs, KeyAction, MouseAction, SetChange, Step, WindowAction},
};

const MAX_STEPS: usize = 50;
const MAX_TEXT_CHARS: usize = 100_000;
pub(crate) const CLICK_COUNTS: RangeInclusive<u8> = 1..=3;
pub(crate) const KEY_REPEATS: RangeInclusive<u32> = 1..=100;
const MAX_KEY_CHORDS: usize = 200;
pub(crate) const SCROLL_AMOUNTS: RangeInclusive<u32> = 1..=50;
pub(crate) const SCREENSHOT_WIDTHS: RangeInclusive<u32> = 64..=2576;
pub(crate) const JPEG_QUALITIES: RangeInclusive<u8> = 1..=100;

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
            Self::Click { count, glide_ms, .. } => {
                check_range("click count", *count, &CLICK_COUNTS).and_then(|()| check_glide(*glide_ms))
            }
            Self::Type { text, .. } | Self::Clipboard { text } => check_text_length("text", text),
            Self::Key { keys, repeat, action, .. } => check_key(keys, *repeat, *action),
            Self::Mouse { action, glide_ms, .. } => self.check_mouse(*action).and_then(|()| check_glide(*glide_ms)),
            Self::Set { .. } => self.set_change().map(drop),
            Self::Window { action, .. } => self.check_window(*action),
            Self::Pick { paths, .. } if paths.is_empty() => Err(Error::invalid("pick needs at least one path")),
            Self::Wait(args) => args.validate().map(drop),
            Self::Notify { title, message } => {
                check_notification_text("title", title, NOTIFY_TITLE_MAX)?;
                check_notification_text("message", message, NOTIFY_MESSAGE_MAX)
            }
            Self::Focus { .. } | Self::Pick { .. } => Ok(()),
        }
    }

    pub fn set_change(&self) -> Result<SetChange> {
        let Self::Set { value, option, checked, expanded, selected, .. } = self else {
            return Err(Error::invalid("only a set step changes a value"));
        };
        let changes: Vec<SetChange> = [
            value.clone().map(SetChange::Value),
            option.clone().map(SetChange::Option),
            checked.map(SetChange::Checked),
            expanded.map(SetChange::Expanded),
            selected.map(SetChange::Selected),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut changes = changes.into_iter();
        match (changes.next(), changes.next()) {
            (Some(SetChange::Value(text)), None) => check_text_length("value", &text).map(|()| SetChange::Value(text)),
            (Some(change), None) => Ok(change),
            _ => Err(Error::invalid("set needs exactly one of value, option, checked, expanded or selected")),
        }
    }

    fn check_mouse(&self, action: MouseAction) -> Result<()> {
        let Self::Mouse { target, to, button, direction, amount, .. } = self else { return Ok(()) };
        let given = [
            ("target", target.is_some()),
            ("to", to.is_some()),
            ("button", button.is_some()),
            ("direction", direction.is_some()),
            ("amount", amount.is_some()),
        ];
        let (allowed, required): (&[&str], &[&str]) = match action {
            MouseAction::Move => (&["target"], &["target"]),
            MouseAction::Down | MouseAction::Up => (&["target", "button"], &[]),
            MouseAction::Scroll => (&["target", "direction", "amount"], &["direction"]),
            MouseAction::Drag => (&["target", "to", "button"], &["target", "to"]),
        };
        check_fields(&format!("mouse action={}", action.name()), &given, allowed, required)?;
        check_optional("scroll amount", *amount, &SCROLL_AMOUNTS)
    }

    fn check_window(&self, action: WindowAction) -> Result<()> {
        let Self::Window { target, app, args, rect, .. } = self else { return Ok(()) };
        let given =
            [("target", target.is_some()), ("app", app.is_some()), ("args", args.is_some()), ("rect", rect.is_some())];
        let (allowed, required): (&[&str], &[&str]) = match action {
            WindowAction::Launch => (&["app", "args"], &["app"]),
            WindowAction::Move => (&["target", "rect"], &["target", "rect"]),
            WindowAction::Close | WindowAction::Minimize | WindowAction::Maximize | WindowAction::Restore => {
                (&["target"], &["target"])
            }
        };
        check_fields(&format!("window action={}", action.name()), &given, allowed, required)?;
        if app.as_deref().is_some_and(|app| app.trim().is_empty()) {
            return Err(Error::invalid("window action=launch needs \"app\""));
        }
        check_screen_rect("rect", *rect)
    }
}

fn check_fields(step: &str, given: &[(&str, bool)], allowed: &[&str], required: &[&str]) -> Result<()> {
    if let Some(name) = first_stray(given, |name| allowed.contains(&name)) {
        return Err(does_not_apply(name, step));
    }
    let missing = required.iter().find(|name| !given.iter().any(|(field, given)| field == *name && *given));
    match missing {
        Some(name) => Err(Error::invalid(format!("{step} needs \"{name}\""))),
        None => Ok(()),
    }
}

fn check_key(keys: &str, repeat: u32, action: KeyAction) -> Result<()> {
    check_range("key repeat", repeat, &KEY_REPEATS)?;
    let chords = parse_keys(keys)?;
    if chords.len() > MAX_KEY_CHORDS {
        return Err(Error::invalid(format!(
            "key takes at most {MAX_KEY_CHORDS} chords per step, got {}; split them across steps, or use type for text",
            chords.len()
        )));
    }
    if action == KeyAction::Press {
        return Ok(());
    }
    if repeat != 1 {
        return Err(Error::invalid(format!("key action={} acts once; drop repeat", action.name())));
    }
    if chords.len() != 1 {
        return Err(Error::invalid(format!(
            "key action={} takes one chord like shift or ctrl+a, got {}",
            action.name(),
            chords.len()
        )));
    }
    Ok(())
}

fn check_range<T: PartialOrd + Display + Copy>(what: &str, value: T, allowed: &RangeInclusive<T>) -> Result<()> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(Error::invalid(format!("{what} must be from {} to {}, got {value}", allowed.start(), allowed.end())))
}

fn check_glide(glide_ms: Option<u64>) -> Result<()> {
    match glide_ms {
        Some(ms) if ms > MAX_GLIDE_MS => {
            Err(Error::invalid(format!("glide_ms must be at most {MAX_GLIDE_MS}, got {ms}")))
        }
        _ => Ok(()),
    }
}

fn check_text_length(what: &str, text: &str) -> Result<()> {
    if text.chars().nth(MAX_TEXT_CHARS).is_none() {
        return Ok(());
    }
    Err(Error::invalid(format!("{what} is longer than {MAX_TEXT_CHARS} characters; split it across steps")))
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

fn check_optional<T: PartialOrd + Display + Copy>(
    what: &str,
    value: Option<T>,
    allowed: &RangeInclusive<T>,
) -> Result<()> {
    value.map_or(Ok(()), |value| check_range(what, value, allowed))
}

fn check_notification_text(field: &str, text: &str, max: usize) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::invalid(format!("{field} must not be empty")));
    }
    if text.contains('\0') {
        return Err(Error::invalid(format!("{field} must not contain NUL characters")));
    }
    let length = text.encode_utf16().count();
    if length > max {
        return Err(Error::invalid(format!(
            "{field} must be at most {max} characters (emoji count as two), got {length}; shorten it"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::ErrorCode,
        steps::{AfterMode, TypeHow},
    };

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    pub(super) fn invalid_message(result: Result<impl std::fmt::Debug>) -> String {
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    fn notify(title: &str, message: &str) -> Step {
        Step::Notify { title: title.to_string(), message: message.to_string() }
    }

    #[test]
    fn notification_text_fits_the_toast_limits() {
        assert!(notify("Export", "Export finished").validate().is_ok());
        assert!(notify(&"t".repeat(NOTIFY_TITLE_MAX), &"m".repeat(NOTIFY_MESSAGE_MAX)).validate().is_ok());
        assert!(notify("Done", "line one\nline two").validate().is_ok());
        assert!(notify(&"é".repeat(NOTIFY_TITLE_MAX), "ok").validate().is_ok());
        assert!(notify(&"🎉".repeat(31), "ok").validate().is_ok());
    }

    #[test]
    fn notification_text_that_is_too_long_empty_or_nul_is_refused() {
        assert_eq!(
            invalid_message(notify(&"t".repeat(64), "ok").validate()),
            "title must be at most 63 characters (emoji count as two), got 64; shorten it"
        );
        assert_eq!(
            invalid_message(notify("Done", &"m".repeat(256)).validate()),
            "message must be at most 255 characters (emoji count as two), got 256; shorten it"
        );
        assert!(invalid_message(notify(&"🎉".repeat(32), "ok").validate()).ends_with("got 64; shorten it"));
        assert_eq!(invalid_message(notify("", "ok").validate()), "title must not be empty");
        assert_eq!(invalid_message(notify("Done", "  \n ").validate()), "message must not be empty");
        assert_eq!(invalid_message(notify("Do\0ne", "ok").validate()), "title must not contain NUL characters");
    }

    #[test]
    fn a_key_step_takes_at_most_two_hundred_chords() {
        let keys = |count: usize| vec!["tab"; count].join(" ");
        let at_limit = Step::Key { keys: keys(200), target: None, repeat: 1, action: KeyAction::Press };
        assert!(at_limit.validate().is_ok());
        let over = Step::Key { keys: keys(201), target: None, repeat: 1, action: KeyAction::Press };
        assert_eq!(
            invalid_message(over.validate()),
            "key takes at most 200 chords per step, got 201; split them across steps, or use type for text"
        );
    }

    #[test]
    fn step_parameters_are_range_checked() {
        let cases = [
            (r#"{"do":"click","target":"e1","count":0}"#, "click count must be from 1 to 3, got 0"),
            (r#"{"do":"click","target":"e1","count":4}"#, "click count must be from 1 to 3, got 4"),
            (r#"{"do":"key","keys":"tab","repeat":0}"#, "key repeat must be from 1 to 100, got 0"),
            (r#"{"do":"key","keys":"tab","repeat":101}"#, "key repeat must be from 1 to 100, got 101"),
            (
                r#"{"do":"mouse","action":"scroll","direction":"down","amount":0}"#,
                "scroll amount must be from 1 to 50, got 0",
            ),
            (
                r#"{"do":"mouse","action":"scroll","direction":"down","amount":51}"#,
                "scroll amount must be from 1 to 50, got 51",
            ),
            (r#"{"do":"wait","until":"text"}"#, "until=text needs \"text\""),
            (
                r#"{"do":"wait","ms":60000}"#,
                "a plain wait pauses at most 5000 ms, got 60000; use until to wait for a change",
            ),
            (
                r#"{"do":"mouse","action":"move","target":"e1","glide_ms":3001}"#,
                "glide_ms must be at most 3000, got 3001",
            ),
            (r#"{"do":"click","target":"e1","glide_ms":9000}"#, "glide_ms must be at most 3000, got 9000"),
        ];
        for (json, expected) in cases {
            assert_eq!(invalid_message(step(json).validate()), expected, "{json}");
        }
        for json in [
            r#"{"do":"click","target":"e1","count":3}"#,
            r#"{"do":"key","keys":"tab","repeat":100}"#,
            r#"{"do":"mouse","action":"scroll","direction":"up","amount":50}"#,
            r#"{"do":"mouse","action":"scroll","direction":"up"}"#,
            r#"{"do":"wait","ms":5000}"#,
            r#"{"do":"mouse","action":"move","target":"e1","glide_ms":3000}"#,
            r#"{"do":"mouse","action":"down","target":"e1","button":"right","glide_ms":0}"#,
            r#"{"do":"mouse","action":"up"}"#,
            r#"{"do":"mouse","action":"drag","target":"e1","to":"e2"}"#,
        ] {
            assert!(step(json).validate().is_ok(), "{json}");
        }
    }

    #[test]
    fn mouse_actions_take_only_their_own_fields() {
        let cases = [
            (r#"{"do":"mouse","action":"move"}"#, "mouse action=move needs \"target\""),
            (
                r#"{"do":"mouse","action":"move","target":"e1","button":"left"}"#,
                "button does not apply to mouse action=move",
            ),
            (r#"{"do":"mouse","action":"scroll"}"#, "mouse action=scroll needs \"direction\""),
            (
                r#"{"do":"mouse","action":"scroll","direction":"up","to":"e2"}"#,
                "to does not apply to mouse action=scroll",
            ),
            (r#"{"do":"mouse","action":"drag","target":"e1"}"#, "mouse action=drag needs \"to\""),
            (r#"{"do":"mouse","action":"down","amount":3}"#, "amount does not apply to mouse action=down"),
        ];
        for (json, expected) in cases {
            assert_eq!(invalid_message(step(json).validate()), expected, "{json}");
        }
    }

    #[test]
    fn held_keys_are_one_chord_pressed_once() {
        assert!(step(r#"{"do":"key","keys":"shift","action":"down"}"#).validate().is_ok());
        assert!(step(r#"{"do":"key","keys":"ctrl+a","action":"up"}"#).validate().is_ok());
        assert_eq!(
            invalid_message(step(r#"{"do":"key","keys":"ctrl+k ctrl+s","action":"down"}"#).validate()),
            "key action=down takes one chord like shift or ctrl+a, got 2"
        );
        assert_eq!(
            invalid_message(step(r#"{"do":"key","keys":"shift","action":"down","repeat":2}"#).validate()),
            "key action=down acts once; drop repeat"
        );
    }

    #[test]
    fn set_needs_exactly_one_change() {
        assert_eq!(
            step(r#"{"do":"set","target":"e1","option":"Blue"}"#).set_change().unwrap(),
            SetChange::Option("Blue".to_string())
        );
        assert_eq!(
            step(r#"{"do":"set","target":"e1","selected":false}"#).set_change().unwrap(),
            SetChange::Selected(false)
        );
        for json in [r#"{"do":"set","target":"e1"}"#, r#"{"do":"set","target":"e1","value":"a","checked":true}"#] {
            assert_eq!(
                invalid_message(step(json).validate()),
                "set needs exactly one of value, option, checked, expanded or selected",
                "{json}"
            );
        }
        let long = Step::Set {
            target: crate::target::TargetSpec::Text("e1".to_string()),
            value: Some("v".repeat(MAX_TEXT_CHARS + 1)),
            option: None,
            checked: None,
            expanded: None,
            selected: None,
        };
        assert!(invalid_message(long.validate()).starts_with("value is longer"));
    }

    #[test]
    fn window_actions_take_their_own_fields() {
        let cases = [
            (r#"{"do":"window","action":"launch"}"#, "window action=launch needs \"app\""),
            (r#"{"do":"window","action":"launch","app":"  "}"#, "window action=launch needs \"app\""),
            (
                r#"{"do":"window","action":"launch","app":"x","target":"w1"}"#,
                "target does not apply to window action=launch",
            ),
            (r#"{"do":"window","action":"move","target":"w1"}"#, "window action=move needs \"rect\""),
            (r#"{"do":"window","action":"close"}"#, "window action=close needs \"target\""),
            (
                r#"{"do":"window","action":"close","target":"w1","app":"x"}"#,
                "app does not apply to window action=close",
            ),
            (
                r#"{"do":"window","action":"move","target":"w1","rect":{"x":0,"y":0,"width":-5,"height":10}}"#,
                "rect 0,0 -5x10 needs a positive size and must stay within ±100000 pixels",
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(invalid_message(step(json).validate()), expected, "{json}");
        }
        assert!(step(r#"{"do":"window","action":"launch","app":"Notepad","args":"a.txt"}"#).validate().is_ok());
    }

    #[test]
    fn typed_and_clipboard_text_length_is_capped() {
        let at_limit = Step::Type { target: None, text: "é".repeat(MAX_TEXT_CHARS), enter: false, how: TypeHow::Auto };
        assert!(at_limit.validate().is_ok());
        let over = Step::Type { target: None, text: "a".repeat(MAX_TEXT_CHARS + 1), enter: false, how: TypeHow::Auto };
        assert_eq!(invalid_message(over.validate()), "text is longer than 100000 characters; split it across steps");
        assert!(Step::Clipboard { text: "a".repeat(MAX_TEXT_CHARS + 1) }.validate().is_err());
        assert_eq!(invalid_message(step(r#"{"do":"pick","paths":[]}"#).validate()), "pick needs at least one path");
    }

    #[test]
    fn act_validation_checks_step_count_and_names_the_failing_step() {
        let pause = step(r#"{"do":"wait","ms":1}"#);
        let at_limit = ActArgs {
            steps: vec![pause.clone(); MAX_STEPS],
            after: AfterMode::Diff,
            stop_on_error: true,
            background: None,
        };
        assert!(at_limit.validate().is_ok());
        let over = ActArgs { steps: vec![pause.clone(); MAX_STEPS + 1], ..at_limit.clone() };
        assert_eq!(invalid_message(over.validate()), "act takes at most 50 steps, got 51");
        let bad = ActArgs { steps: vec![pause, step(r#"{"do":"click","target":"e1","count":9}"#)], ..at_limit };
        assert_eq!(invalid_message(bad.validate()), "step 2: click count must be from 1 to 3, got 9");
    }
}
