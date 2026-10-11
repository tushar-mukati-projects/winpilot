use std::{fmt::Display, ops::RangeInclusive};

use crate::{
    error::{Error, Result},
    geometry::{COORD_LIMIT, Rect},
    glide::MAX_GLIDE_MS,
    protocol::{
        ActArgs, NOTIFY_MESSAGE_MAX, NOTIFY_TITLE_MAX, NotifyArgs, ReadTextArgs, ScreenshotArgs, Step, WindowsArgs,
    },
    record_plan::{RECORD_SECONDS, RECORD_WIDTHS},
    recording::{RecordAction, RecordArgs, VideoFormat},
};

const MAX_STEPS: usize = 50;
const MAX_TEXT_CHARS: usize = 100_000;
const MAX_WAIT_STEP_MS: u64 = 60_000;
const CLICK_COUNTS: RangeInclusive<u8> = 1..=3;
const KEY_REPEATS: RangeInclusive<u32> = 1..=100;
const SCROLL_AMOUNTS: RangeInclusive<u32> = 1..=50;
pub const SCREENSHOT_WIDTHS: RangeInclusive<u32> = 64..=2576;
const JPEG_QUALITIES: RangeInclusive<u8> = 1..=100;
const EMPTY_RECORD_PATH: &str = r"path is empty; pass a full path like C:\clips\demo.mp4 or leave it out";

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
            Self::Type { text, .. } => check_text_length("text", text),
            Self::SetValue { value, .. } => check_text_length("value", value),
            Self::Key { repeat, .. } => check_range("key repeat", *repeat, &KEY_REPEATS),
            Self::Scroll { amount, glide_ms, .. } => {
                check_range("scroll amount", *amount, &SCROLL_AMOUNTS).and_then(|()| check_glide(*glide_ms))
            }
            Self::WaitFor(condition) => condition.validate().map(drop),
            Self::Wait { ms } => check_wait(*ms),
            Self::Hover { glide_ms, .. }
            | Self::Drag { glide_ms, .. }
            | Self::Move { glide_ms, .. }
            | Self::MouseDown { glide_ms, .. }
            | Self::MouseUp { glide_ms, .. } => check_glide(*glide_ms),
            Self::Select { .. }
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

fn check_optional<T: PartialOrd + Display + Copy>(
    what: &str,
    value: Option<T>,
    allowed: &RangeInclusive<T>,
) -> Result<()> {
    value.map_or(Ok(()), |value| check_range(what, value, allowed))
}

impl ScreenshotArgs {
    pub fn validate(&self) -> Result<()> {
        check_screen_rect("region", self.region)?;
        check_optional("max_width", self.max_width, &SCREENSHOT_WIDTHS)?;
        check_optional("quality", self.quality, &JPEG_QUALITIES)
    }
}

impl RecordArgs {
    pub fn validate(&self) -> Result<()> {
        if self.action != RecordAction::Start {
            return check_no_start_options(self);
        }
        check_one_source(self)?;
        check_screen_rect("region", self.region)?;
        let fps_limit = self.format.unwrap_or(VideoFormat::Mp4).max_fps();
        check_optional("fps", self.fps, &(1..=fps_limit))?;
        check_optional("max_seconds", self.max_seconds, &RECORD_SECONDS)?;
        check_optional("max_width", self.max_width, &RECORD_WIDTHS)?;
        check_record_path(self.path.as_deref(), self.format)
    }
}

fn check_no_start_options(args: &RecordArgs) -> Result<()> {
    let given = args.start_options();
    if given.is_empty() {
        return Ok(());
    }
    let action = if args.action == RecordAction::Stop { "stop" } else { "status" };
    Err(Error::invalid(format!("record action {action} takes no other fields; remove {}", given.join(", "))))
}

fn check_one_source(args: &RecordArgs) -> Result<()> {
    let sources = [args.target.is_some(), args.region.is_some(), args.monitor.is_some()];
    if sources.into_iter().filter(|given| *given).count() <= 1 {
        return Ok(());
    }
    Err(Error::invalid("give only one of target, region or monitor"))
}

fn check_record_path(path: Option<&str>, format: Option<VideoFormat>) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    if path.trim().is_empty() {
        return Err(Error::invalid(EMPTY_RECORD_PATH));
    }
    VideoFormat::for_path(path, format).map(drop)
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

impl NotifyArgs {
    pub fn validate(&self) -> Result<()> {
        check_notification_text("title", &self.title, NOTIFY_TITLE_MAX)?;
        check_notification_text("message", &self.message, NOTIFY_MESSAGE_MAX)
    }
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
        protocol::{AfterMode, TypeHow, WindowAction},
        target::TargetSpec,
    };

    fn notify(title: &str, message: &str) -> NotifyArgs {
        NotifyArgs { title: title.to_string(), message: message.to_string() }
    }

    #[test]
    fn notification_text_fits_the_balloon_limits() {
        assert!(notify("Export", "Export finished").validate().is_ok());
        assert!(notify(&"t".repeat(NOTIFY_TITLE_MAX), &"m".repeat(NOTIFY_MESSAGE_MAX)).validate().is_ok());
        assert!(notify("Done", "line one\nline two").validate().is_ok());
        assert!(notify(&"é".repeat(NOTIFY_TITLE_MAX), "ok").validate().is_ok());
    }

    #[test]
    fn notification_text_that_is_too_long_is_refused_not_cut() {
        assert_eq!(
            invalid_message(notify(&"t".repeat(64), "ok").validate()),
            "title must be at most 63 characters (emoji count as two), got 64; shorten it"
        );
        assert_eq!(
            invalid_message(notify("Done", &"m".repeat(256)).validate()),
            "message must be at most 255 characters (emoji count as two), got 256; shorten it"
        );
        let emoji = "🎉".repeat(32);
        assert_eq!(emoji.chars().count(), 32);
        assert!(invalid_message(notify(&emoji, "ok").validate()).ends_with("got 64; shorten it"));
        assert!(notify(&"🎉".repeat(31), "ok").validate().is_ok());
    }

    #[test]
    fn notification_text_must_be_present_and_free_of_nul() {
        assert_eq!(invalid_message(notify("", "ok").validate()), "title must not be empty");
        assert_eq!(invalid_message(notify("Done", "  \n ").validate()), "message must not be empty");
        assert_eq!(invalid_message(notify("Do\0ne", "ok").validate()), "title must not contain NUL characters");
        assert_eq!(invalid_message(notify("Done", "ok\0hidden").validate()), "message must not contain NUL characters");
    }

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
            (r#"{"do":"move","to":"e1","glide_ms":3001}"#, "glide_ms must be at most 3000, got 3001"),
            (r#"{"do":"click","target":"e1","glide_ms":9000}"#, "glide_ms must be at most 3000, got 9000"),
            (r#"{"do":"drag","from":"e1","to":"e2","glide_ms":3001}"#, "glide_ms must be at most 3000, got 3001"),
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
            r#"{"do":"hover","target":"e1","glide_ms":3000}"#,
            r#"{"do":"mouse_down","target":"e1","glide_ms":0}"#,
            r#"{"do":"mouse_up"}"#,
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

    #[test]
    fn screenshot_width_and_quality_are_range_checked() {
        let shot = |max_width, quality| ScreenshotArgs { max_width, quality, ..ScreenshotArgs::default() };
        assert!(shot(Some(64), Some(1)).validate().is_ok());
        assert!(shot(Some(2576), Some(100)).validate().is_ok());
        assert_eq!(invalid_message(shot(Some(63), None).validate()), "max_width must be from 64 to 2576, got 63");
        assert_eq!(invalid_message(shot(Some(2577), None).validate()), "max_width must be from 64 to 2576, got 2577");
        assert_eq!(invalid_message(shot(None, Some(0)).validate()), "quality must be from 1 to 100, got 0");
        assert_eq!(invalid_message(shot(None, Some(101)).validate()), "quality must be from 1 to 100, got 101");
    }

    fn record(json: &str) -> RecordArgs {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn record_start_options_are_range_checked() {
        let cases = [
            (r#"{"action":"start","fps":0}"#, "fps must be from 1 to 60, got 0"),
            (r#"{"action":"start","fps":61}"#, "fps must be from 1 to 60, got 61"),
            (r#"{"action":"start","format":"gif","fps":21}"#, "fps must be from 1 to 20, got 21"),
            (r#"{"action":"start","max_seconds":0}"#, "max_seconds must be from 1 to 1800, got 0"),
            (r#"{"action":"start","max_seconds":1801}"#, "max_seconds must be from 1 to 1800, got 1801"),
            (r#"{"action":"start","max_width":159}"#, "max_width must be from 160 to 3840, got 159"),
            (r#"{"action":"start","max_width":3841}"#, "max_width must be from 160 to 3840, got 3841"),
            (
                r#"{"action":"start","region":{"x":0,"y":0,"width":0,"height":5}}"#,
                "region 0,0 0x5 needs a positive size and must stay within ±100000 pixels",
            ),
            (r#"{"action":"start","target":"w1","monitor":0}"#, "give only one of target, region or monitor"),
            (r#"{"action":"start","path":"C:\\v\\a.webm"}"#, "path must end in .mp4 or .gif"),
            (
                r#"{"action":"start","path":"  "}"#,
                "path is empty; pass a full path like C:\\clips\\demo.mp4 or leave it out",
            ),
            (
                r#"{"action":"start","path":"C:\\v\\a.gif","format":"mp4"}"#,
                "path ends in .gif but format is mp4; make them match",
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(invalid_message(record(json).validate()), expected, "{json}");
        }
        for json in [
            r#"{"action":"start"}"#,
            r#"{"action":"start","fps":60,"max_seconds":1800,"max_width":3840,"monitor":1}"#,
            r#"{"action":"start","format":"gif","fps":20,"max_width":160,"region":{"x":-100,"y":0,"width":50,"height":50}}"#,
            r#"{"action":"start","path":"C:\\v\\a.GIF","format":"gif","target":"e4"}"#,
            r#"{"action":"start","path":"C:\\v\\a.gif","fps":20}"#,
        ] {
            assert!(record(json).validate().is_ok(), "{json}");
        }
    }

    #[test]
    fn record_stop_and_status_take_no_start_options() {
        assert!(record(r#"{"action":"stop"}"#).validate().is_ok());
        assert!(record(r#"{"action":"status"}"#).validate().is_ok());
        assert_eq!(
            invalid_message(record(r#"{"action":"stop","fps":10,"path":"C:\\a.mp4"}"#).validate()),
            "record action stop takes no other fields; remove fps, path"
        );
        assert_eq!(
            invalid_message(record(r#"{"action":"status","monitor":0}"#).validate()),
            "record action status takes no other fields; remove monitor"
        );
    }
}
