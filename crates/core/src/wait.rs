use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    blank::non_blank,
    error::{Error, Result},
    target::{Target, parse_text_target},
};

pub const DEFAULT_WAIT: Duration = Duration::from_secs(5);
pub const MAX_WAIT: Duration = Duration::from_secs(60);
pub const DEFAULT_IDLE: Duration = Duration::from_millis(300);
pub const MAX_PLAIN_WAIT: Duration = Duration::from_secs(5);
const MAX_WAIT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Until {
    Time,
    Appears,
    Disappears,
    Window,
    WindowClosed,
    Text,
    Idle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitArgs {
    #[serde(default)]
    #[schemars(
        description = "time (the default when only ms is given), appears/disappears (target), window/window_closed (window title), text (text) or idle."
    )]
    pub until: Option<Until>,
    #[serde(default)]
    #[schemars(
        description = "time: milliseconds to pause, capped at 5000. idle: how long the UI must stay quiet (default 300)."
    )]
    pub ms: Option<u64>,
    #[serde(default)]
    #[schemars(description = "appears/disappears: the element ref or selector to watch.")]
    pub target: Option<String>,
    #[serde(default)]
    #[schemars(description = "text: the text to wait for.")]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(description = "window/window_closed: window title text. appears/disappears/text: the window to watch.")]
    pub window: Option<String>,
    #[serde(default)]
    #[schemars(description = "Give up after this long (default 5000, max 60000).")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitCondition {
    Time { wanted: Duration },
    Appears { target: Target, window: Option<String> },
    Disappears { target: Target, window: Option<String> },
    WindowOpens { title: String },
    WindowCloses { title: String },
    Text { text: String, window: Option<String> },
    Idle { quiet: Duration },
}

impl Until {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Appears => "appears",
            Self::Disappears => "disappears",
            Self::Window => "window",
            Self::WindowClosed => "window_closed",
            Self::Text => "text",
            Self::Idle => "idle",
        }
    }

    const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Time => &["ms"],
            Self::Appears | Self::Disappears => &["target", "window", "timeout_ms"],
            Self::Window | Self::WindowClosed => &["window", "timeout_ms"],
            Self::Text => &["text", "window", "timeout_ms"],
            Self::Idle => &["ms", "timeout_ms"],
        }
    }
}

impl WaitArgs {
    pub fn until(&self) -> Until {
        self.until.unwrap_or(Until::Time)
    }

    pub fn timeout(&self) -> Duration {
        self.timeout_ms.map_or(DEFAULT_WAIT, Duration::from_millis).min(MAX_WAIT)
    }

    pub fn validate(&self) -> Result<WaitCondition> {
        let until = self.until();
        self.check_fields(until)?;
        match until {
            Until::Time => self.time(),
            Until::Appears => Ok(WaitCondition::Appears { target: self.required_target()?, window: self.scope() }),
            Until::Disappears => {
                Ok(WaitCondition::Disappears { target: self.required_target()?, window: self.scope() })
            }
            Until::Window => Ok(WaitCondition::WindowOpens { title: self.required(self.window.as_ref(), "window")? }),
            Until::WindowClosed => {
                Ok(WaitCondition::WindowCloses { title: self.required(self.window.as_ref(), "window")? })
            }
            Until::Text => {
                Ok(WaitCondition::Text { text: self.required(self.text.as_ref(), "text")?, window: self.scope() })
            }
            Until::Idle => self.idle(),
        }
    }

    fn given(&self) -> [(&'static str, bool); 5] {
        [
            ("ms", self.ms.is_some()),
            ("target", self.target.is_some()),
            ("text", self.text.is_some()),
            ("window", self.window.is_some()),
            ("timeout_ms", self.timeout_ms.is_some()),
        ]
    }

    fn check_fields(&self, until: Until) -> Result<()> {
        if self.until.is_none() && self.ms.is_none() {
            return Err(Error::invalid("wait needs ms (to pause) or until (to wait for a change)"));
        }
        let stray = self.given().into_iter().find(|(name, given)| *given && !until.fields().contains(name));
        match stray {
            Some((name, _)) => Err(Error::invalid(format!("{name} does not apply to wait until={}", until.name()))),
            None => Ok(()),
        }
    }

    fn time(&self) -> Result<WaitCondition> {
        let ms = self.ms.ok_or_else(|| Error::invalid("until=time needs \"ms\""))?;
        if ms > MAX_WAIT_MS {
            return Err(Error::invalid(format!(
                "wait ms must be at most {MAX_WAIT_MS}, got {ms}; use until to wait for a change"
            )));
        }
        Ok(WaitCondition::Time { wanted: Duration::from_millis(ms) })
    }

    fn idle(&self) -> Result<WaitCondition> {
        let quiet = self.ms.map_or(DEFAULT_IDLE, Duration::from_millis);
        let timeout = self.timeout();
        if quiet >= timeout {
            return Err(Error::invalid(format!(
                "idle ms must be shorter than timeout_ms (quiet {} ms, timeout {} ms)",
                quiet.as_millis(),
                timeout.as_millis()
            )));
        }
        Ok(WaitCondition::Idle { quiet })
    }

    fn scope(&self) -> Option<String> {
        non_blank(self.window.as_ref())
    }

    fn required_target(&self) -> Result<Target> {
        parse_text_target(&self.required(self.target.as_ref(), "target")?)
    }

    fn required(&self, value: Option<&String>, field: &str) -> Result<String> {
        non_blank(value).ok_or_else(|| Error::invalid(format!("until={} needs \"{field}\"", self.until().name())))
    }
}

pub fn plain_wait(wanted: Duration) -> (Duration, bool) {
    (wanted.min(MAX_PLAIN_WAIT), wanted > MAX_PLAIN_WAIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ErrorCode, refs::ElementRef};

    fn wait(json: &str) -> WaitArgs {
        serde_json::from_str(json).unwrap()
    }

    fn invalid(json: &str) -> String {
        let error = wait(json).validate().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{json}");
        error.message
    }

    #[test]
    fn ms_alone_is_a_plain_pause() {
        assert_eq!(
            wait(r#"{"ms":250}"#).validate().unwrap(),
            WaitCondition::Time { wanted: Duration::from_millis(250) }
        );
        assert_eq!(
            wait(r#"{"until":"time","ms":60000}"#).validate().unwrap(),
            WaitCondition::Time { wanted: MAX_WAIT }
        );
        assert_eq!(invalid(r#"{"until":"time"}"#), "until=time needs \"ms\"");
        assert_eq!(invalid("{}"), "wait needs ms (to pause) or until (to wait for a change)");
        assert_eq!(
            invalid(r#"{"ms":60001}"#),
            "wait ms must be at most 60000, got 60001; use until to wait for a change"
        );
        assert_eq!(invalid(r#"{"ms":10,"timeout_ms":100}"#), "timeout_ms does not apply to wait until=time");
    }

    #[test]
    fn plain_waits_are_capped_at_five_seconds() {
        assert_eq!(plain_wait(Duration::ZERO), (Duration::ZERO, false));
        assert_eq!(plain_wait(Duration::from_millis(500)), (Duration::from_millis(500), false));
        assert_eq!(plain_wait(MAX_PLAIN_WAIT), (MAX_PLAIN_WAIT, false));
        assert_eq!(plain_wait(Duration::from_millis(5001)), (MAX_PLAIN_WAIT, true));
    }

    #[test]
    fn conditions_require_their_fields() {
        assert_eq!(invalid(r#"{"until":"text"}"#), "until=text needs \"text\"");
        assert_eq!(invalid(r#"{"until":"appears","target":"   "}"#), "until=appears needs \"target\"");
        assert_eq!(invalid(r#"{"until":"window_closed","window":" "}"#), "until=window_closed needs \"window\"");
        assert_eq!(
            invalid(r#"{"until":"window","window":"Save","target":"e1"}"#),
            "target does not apply to wait until=window"
        );
        assert_eq!(invalid(r#"{"until":"idle","text":"x"}"#), "text does not apply to wait until=idle");
    }

    #[test]
    fn blank_scopes_count_as_missing() {
        assert_eq!(
            wait(r#"{"until":"appears","target":"e1","window":"  "}"#).validate().unwrap(),
            WaitCondition::Appears { target: Target::Element(ElementRef(1)), window: None }
        );
        assert_eq!(
            wait(r#"{"until":"text","text":"Done","window":""}"#).validate().unwrap(),
            WaitCondition::Text { text: "Done".to_string(), window: None }
        );
        let watched = wait(r#"{"until":"disappears","target":"e2","window":" Notepad "}"#).validate().unwrap();
        assert!(matches!(watched, WaitCondition::Disappears { window: Some(title), .. } if title == "Notepad"));
    }

    #[test]
    fn idle_quiet_time_must_be_shorter_than_the_timeout() {
        assert_eq!(
            invalid(r#"{"until":"idle","ms":30000}"#),
            "idle ms must be shorter than timeout_ms (quiet 30000 ms, timeout 5000 ms)"
        );
        assert!(wait(r#"{"until":"idle","ms":2000,"timeout_ms":2000}"#).validate().is_err());
        assert!(wait(r#"{"until":"idle","timeout_ms":200}"#).validate().is_err());
        assert_eq!(
            wait(r#"{"until":"idle","ms":1999,"timeout_ms":2000}"#).validate().unwrap(),
            WaitCondition::Idle { quiet: Duration::from_millis(1999) }
        );
        assert_eq!(wait(r#"{"until":"idle"}"#).validate().unwrap(), WaitCondition::Idle { quiet: DEFAULT_IDLE });
    }

    #[test]
    fn timeouts_default_and_cap() {
        assert_eq!(wait(r#"{"until":"idle"}"#).timeout(), DEFAULT_WAIT);
        assert_eq!(wait(r#"{"until":"idle","timeout_ms":999999}"#).timeout(), MAX_WAIT);
        assert_eq!(wait(r#"{"ms":5}"#).until(), Until::Time);
    }
}
