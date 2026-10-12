use serde::{Deserialize, Serialize};

pub use crate::{
    budget::pick_wait,
    observe::{ObserveArgs, ObserveMode, OcrMode},
    observe_request::{
        AppsRequest, ClipboardRequest, ElementsRequest, ImageFormat, ObserveRequest, ScreenshotRequest, SearchRequest,
        TextRequest,
    },
    quality::Quality,
    recording::{RecordAction, RecordReport, RecordRequest, RecordState, VideoFormat},
    reports::{
        ActReport, After, AppInfo, AppKind, AppsReport, ClipboardReport, DiffReport, Found, MonitorInfo,
        MonitorsReport, Observation, ScreenshotReport, SearchReport, StatusReport, StepOutcome, TextReport, TextSource,
        Timing, WindowsReport,
    },
    steps::{
        ActArgs, AfterMode, ClickHow, Direction, KeyAction, MouseAction, MouseButton, SetChange, Step, TypeHow,
        WindowAction,
    },
    wait::{Until, WaitArgs, WaitCondition},
};

pub const NOTIFY_TITLE_MAX: usize = 63;
pub const NOTIFY_MESSAGE_MAX: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Call {
    Observe(ObserveArgs),
    Act(ActArgs),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Observation(Observation),
    Search(SearchReport),
    Text(TextReport),
    Screenshot(ScreenshotReport),
    Record(RecordReport),
    Windows(WindowsReport),
    Monitors(MonitorsReport),
    Apps(AppsReport),
    Clipboard(ClipboardReport),
    Status(StatusReport),
    Act(ActReport),
}

impl Call {
    pub const fn tool(&self) -> &'static str {
        match self {
            Self::Observe(_) => "observe",
            Self::Act(_) => "act",
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Observe(args) => args.mode.label(),
            Self::Act(_) => "act",
        }
    }

    pub fn is_status(&self) -> bool {
        matches!(self, Self::Observe(args) if args.mode == ObserveMode::Status)
    }

    pub fn status() -> Self {
        Self::Observe(ObserveArgs { mode: ObserveMode::Status, ..ObserveArgs::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_name_their_tool_and_mode() {
        let shot = Call::Observe(ObserveArgs { mode: ObserveMode::Screenshot, ..ObserveArgs::default() });
        assert_eq!((shot.tool(), shot.label()), ("observe", "observe screenshot"));
        let act =
            Call::Act(ActArgs { steps: Vec::new(), after: AfterMode::None, stop_on_error: true, background: None });
        assert_eq!((act.tool(), act.label()), ("act", "act"));
    }

    #[test]
    fn only_the_status_mode_is_status() {
        assert!(Call::status().is_status());
        assert!(!Call::Observe(ObserveArgs::default()).is_status());
    }

    #[test]
    fn calls_round_trip_through_json() {
        let call: Call = serde_json::from_str(r#"{"observe":{"mode":"text","target":"e4"}}"#).unwrap();
        let Call::Observe(args) = &call else { panic!("expected observe") };
        assert_eq!((args.mode, args.target.as_deref()), (ObserveMode::Text, Some("e4")));
        let back: Call = serde_json::from_str(&serde_json::to_string(&call).unwrap()).unwrap();
        assert_eq!(back, call);
    }
}
