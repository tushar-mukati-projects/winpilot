use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    diff::Change,
    element::{Element, Snapshot, WindowInfo},
    error::{Error, Result},
    geometry::{ImageMapping, Rect},
    refs::ImageRef,
    target::{Target, TargetSpec, parse_text_target},
};

const fn default_true() -> bool {
    true
}

const fn default_one() -> u32 {
    1
}

const fn default_click_count() -> u8 {
    1
}

const fn default_scroll_amount() -> u32 {
    3
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ObserveArgs {
    #[schemars(
        description = "Window to read: a window ref like w2032546 or a selector like window:\"Notepad\". Default: the foreground window."
    )]
    pub window: Option<String>,
    #[schemars(description = "Only list elements matching this selector, plus their ancestors.")]
    pub query: Option<String>,
    #[schemars(description = "Also list non-interactive elements such as static text and panes.")]
    pub all: bool,
    #[schemars(description = "Add @x,y,w,h screen rectangles to each element.")]
    pub coords: bool,
    #[schemars(description = "Maximum elements to list (default 300).")]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindArgs {
    #[schemars(description = "Selector or plain text, e.g. role:button name~:save or Save.")]
    pub query: String,
    #[serde(default)]
    #[schemars(description = "Window to search: a window ref or selector. Default: the foreground window.")]
    pub window: Option<String>,
    #[serde(default)]
    #[schemars(description = "Search every app window instead of one.")]
    pub all_windows: bool,
    #[serde(default)]
    #[schemars(description = "Maximum matches (default 20).")]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Jpeg,
    Png,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ScreenshotArgs {
    #[schemars(description = "Crop to a window or element: a ref or selector. Default: the foreground window.")]
    pub target: Option<String>,
    #[schemars(description = "Crop to this screen rectangle in physical pixels instead.")]
    pub region: Option<Rect>,
    #[schemars(description = "Capture a whole monitor by index (0 = primary).")]
    pub monitor: Option<u32>,
    #[schemars(description = "Maximum image width in pixels (default 1280).")]
    pub max_width: Option<u32>,
    pub format: ImageFormat,
    #[schemars(description = "Draw element ref labels (e12) on interactive elements.")]
    pub marks: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AfterMode {
    #[default]
    Diff,
    Observe,
    Screenshot,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActArgs {
    #[schemars(description = "Steps run in order. Each is an object with a \"do\" field.")]
    pub steps: Vec<Step>,
    #[serde(rename = "return", default)]
    #[schemars(description = "What to report afterwards: diff (default), observe, screenshot or none.")]
    pub after: AfterMode,
    #[serde(default = "default_true")]
    #[schemars(description = "Stop at the first failing step (default true).")]
    pub stop_on_error: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClickHow {
    #[default]
    Auto,
    Invoke,
    Mouse,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TypeHow {
    #[default]
    Auto,
    Keys,
    Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Step {
    Click {
        target: TargetSpec,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        count: u8,
        #[serde(default)]
        #[schemars(description = "Modifier keys to hold, e.g. ctrl or ctrl+shift.")]
        modifiers: Option<String>,
        #[serde(default)]
        how: ClickHow,
    },
    Hover {
        target: TargetSpec,
    },
    Type {
        #[serde(default)]
        target: Option<TargetSpec>,
        text: String,
        #[serde(default)]
        #[schemars(description = "Press Enter after typing.")]
        enter: bool,
        #[serde(default)]
        how: TypeHow,
    },
    SetValue {
        target: TargetSpec,
        value: String,
    },
    Key {
        #[schemars(description = "Keys like ctrl+s, enter, alt+f4, or a sequence: ctrl+k ctrl+s.")]
        keys: String,
        #[serde(default)]
        target: Option<TargetSpec>,
        #[serde(default = "default_one")]
        repeat: u32,
    },
    Scroll {
        #[serde(default)]
        target: Option<TargetSpec>,
        direction: Direction,
        #[serde(default = "default_scroll_amount")]
        amount: u32,
    },
    Drag {
        from: TargetSpec,
        to: TargetSpec,
    },
    Select {
        target: TargetSpec,
        #[serde(default)]
        #[schemars(description = "Item to pick inside a combobox or list; omit to select the target itself.")]
        option: Option<String>,
    },
    Toggle {
        target: TargetSpec,
        #[serde(default)]
        #[schemars(description = "Desired state; omit to flip.")]
        on: Option<bool>,
    },
    Expand {
        target: TargetSpec,
    },
    Collapse {
        target: TargetSpec,
    },
    Focus {
        target: TargetSpec,
    },
    WaitFor(Condition),
    Wait {
        ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Until {
    Appears,
    Disappears,
    Window,
    WindowClosed,
    Text,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    #[schemars(
        description = "appears/disappears (needs target), window/window_closed (needs window title text), text (needs text), idle (UI quiet for ms)."
    )]
    pub until: Until,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(description = "Window title text for window/window_closed, or the window to watch for the others.")]
    pub window: Option<String>,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    #[schemars(description = "Give up after this long (default 5000, max 60000).")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitCondition {
    Appears { target: Target, window: Option<String> },
    Disappears { target: Target, window: Option<String> },
    WindowOpens { title: String },
    WindowCloses { title: String },
    Text { text: String, window: Option<String> },
    Idle { quiet: Duration },
}

pub const DEFAULT_WAIT: Duration = Duration::from_secs(5);
pub const MAX_WAIT: Duration = Duration::from_secs(60);
pub const DEFAULT_IDLE: Duration = Duration::from_millis(300);
pub const MAX_PLAIN_WAIT: Duration = Duration::from_secs(5);

impl Condition {
    pub fn timeout(&self) -> Duration {
        self.timeout_ms.map_or(DEFAULT_WAIT, Duration::from_millis).min(MAX_WAIT)
    }

    pub fn validate(&self) -> Result<WaitCondition> {
        let required = |value: &Option<String>, field: &str| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .ok_or_else(|| Error::invalid(format!("until={} needs \"{field}\"", until_name(self.until))))
        };
        Ok(match self.until {
            Until::Appears => WaitCondition::Appears {
                target: parse_text_target(&required(&self.target, "target")?)?,
                window: self.window.clone(),
            },
            Until::Disappears => WaitCondition::Disappears {
                target: parse_text_target(&required(&self.target, "target")?)?,
                window: self.window.clone(),
            },
            Until::Window => WaitCondition::WindowOpens { title: required(&self.window, "window")? },
            Until::WindowClosed => WaitCondition::WindowCloses { title: required(&self.window, "window")? },
            Until::Text => WaitCondition::Text { text: required(&self.text, "text")?, window: self.window.clone() },
            Until::Idle => {
                WaitCondition::Idle { quiet: self.ms.map_or(DEFAULT_IDLE, Duration::from_millis).min(MAX_WAIT) }
            }
        })
    }
}

const fn until_name(until: Until) -> &'static str {
    match until {
        Until::Appears => "appears",
        Until::Disappears => "disappears",
        Until::Window => "window",
        Until::WindowClosed => "window_closed",
        Until::Text => "text",
        Until::Idle => "idle",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    List,
    Focus,
    Launch,
    Close,
    Minimize,
    Maximize,
    Restore,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowsArgs {
    pub action: WindowAction,
    #[serde(default)]
    #[schemars(description = "Window ref or selector for focus/close/minimize/maximize/restore/move.")]
    pub window: Option<String>,
    #[serde(default)]
    #[schemars(description = "App to launch: a Start-menu name like Notepad, an app id, or an exe path.")]
    pub app: Option<String>,
    #[serde(default)]
    #[schemars(description = "Command-line arguments for launch.")]
    pub args: Option<String>,
    #[serde(default)]
    #[schemars(description = "New screen rectangle for move.")]
    pub rect: Option<Rect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickFileArgs {
    #[schemars(description = "Absolute file paths to pick. Several files must share one folder.")]
    pub paths: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Click this first to open the dialog, e.g. an Upload button.")]
    pub open_with: Option<TargetSpec>,
    #[serde(default)]
    #[schemars(description = "In a Save dialog, confirm replacing an existing file.")]
    pub overwrite: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextMethod {
    #[default]
    Auto,
    Uia,
    Ocr,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ReadTextArgs {
    #[schemars(description = "Element or window ref or selector. Default: the foreground window.")]
    pub target: Option<String>,
    #[schemars(description = "Screen rectangle to OCR instead.")]
    pub region: Option<Rect>,
    pub method: TextMethod,
    #[schemars(description = "Maximum characters (default 20000).")]
    pub max_chars: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardAction {
    Get,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipboardArgs {
    pub action: ClipboardAction,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusArgs {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub snapshot: Snapshot,
    pub others: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub element: Element,
    pub window: WindowInfo,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindReport {
    pub matches: Vec<Found>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotReport {
    pub image: ImageRef,
    pub mime: String,
    pub data: String,
    pub width: u32,
    pub height: u32,
    pub mapping: ImageMapping,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOutcome {
    pub index: usize,
    pub step: String,
    pub method: Option<String>,
    pub note: Option<String>,
    pub error: Option<Error>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffReport {
    pub window: WindowInfo,
    pub focused: Option<Element>,
    pub changes: Vec<Change>,
    pub opened: Vec<WindowInfo>,
    pub closed: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum After {
    None,
    Diff(DiffReport),
    Observation(Observation),
    Screenshot(ScreenshotReport),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActReport {
    pub steps: Vec<StepOutcome>,
    pub total_steps: usize,
    pub after: After,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitReport {
    pub satisfied: bool,
    pub elapsed_ms: u64,
    pub detail: String,
    pub element: Option<Element>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsReport {
    pub windows: Vec<WindowInfo>,
    pub window: Option<WindowInfo>,
    pub observation: Option<Observation>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogKind {
    Open,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickReport {
    pub dialog: DialogKind,
    pub files: Vec<String>,
    pub after: Option<Observation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSource {
    Uia,
    Ocr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextReport {
    pub text: String,
    pub source: TextSource,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardReport {
    pub text: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    pub tool: String,
    pub ms: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReport {
    pub version: String,
    pub uptime_ms: u64,
    pub stopped: bool,
    pub user_idle_ms: Option<u64>,
    pub kill_hotkey: String,
    pub hotkey_registered: bool,
    pub elevated: bool,
    pub secure_desktop: bool,
    pub worker_restarts: u32,
    pub recent: Vec<Timing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Call {
    Observe(ObserveArgs),
    Find(FindArgs),
    Screenshot(ScreenshotArgs),
    Act(ActArgs),
    WaitFor(Condition),
    Windows(WindowsArgs),
    PickFile(PickFileArgs),
    ReadText(ReadTextArgs),
    Clipboard(ClipboardArgs),
    Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Observe(Observation),
    Find(FindReport),
    Screenshot(ScreenshotReport),
    Act(ActReport),
    WaitFor(WaitReport),
    Windows(WindowsReport),
    PickFile(PickReport),
    ReadText(TextReport),
    Clipboard(ClipboardReport),
    Status(StatusReport),
}

const STEP_BUDGET: Duration = Duration::from_secs(6);
const ACT_OVERHEAD: Duration = Duration::from_secs(10);
const MAX_DEADLINE: Duration = Duration::from_secs(180);
const DEFAULT_PICK: Duration = Duration::from_secs(15);

impl Call {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Observe(_) => "observe",
            Self::Find(_) => "find",
            Self::Screenshot(_) => "screenshot",
            Self::Act(_) => "act",
            Self::WaitFor(_) => "wait_for",
            Self::Windows(_) => "windows",
            Self::PickFile(_) => "pick_file",
            Self::ReadText(_) => "read_text",
            Self::Clipboard(_) => "clipboard",
            Self::Status => "status",
        }
    }

    pub fn is_read_only(&self) -> bool {
        match self {
            Self::Observe(_)
            | Self::Find(_)
            | Self::Screenshot(_)
            | Self::WaitFor(_)
            | Self::ReadText(_)
            | Self::Status => true,
            Self::Windows(args) => args.action == WindowAction::List,
            Self::Clipboard(args) => args.action == ClipboardAction::Get,
            Self::Act(_) | Self::PickFile(_) => false,
        }
    }

    pub fn deadline(&self) -> Duration {
        let deadline = match self {
            Self::Observe(_) | Self::Find(_) | Self::Windows(WindowsArgs { action: WindowAction::List, .. }) => {
                Duration::from_secs(10)
            }
            Self::Screenshot(_) => Duration::from_secs(8),
            Self::ReadText(_) => Duration::from_secs(20),
            Self::Status => Duration::from_secs(3),
            Self::Clipboard(_) => Duration::from_secs(5),
            Self::Windows(WindowsArgs { action: WindowAction::Launch, .. }) => Duration::from_secs(30),
            Self::Windows(_) => Duration::from_secs(10),
            Self::WaitFor(condition) => condition.timeout() + Duration::from_secs(5),
            Self::PickFile(args) => {
                args.timeout_ms.map_or(DEFAULT_PICK, Duration::from_millis).min(MAX_WAIT) + ACT_OVERHEAD
            }
            Self::Act(args) => ACT_OVERHEAD + args.steps.iter().map(step_budget).sum::<Duration>(),
        };
        deadline.min(MAX_DEADLINE)
    }
}

fn step_budget(step: &Step) -> Duration {
    match step {
        Step::WaitFor(condition) => condition.timeout() + Duration::from_secs(1),
        Step::Wait { ms } => Duration::from_millis(*ms).min(MAX_PLAIN_WAIT) + Duration::from_secs(1),
        _ => STEP_BUDGET,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_deserialize_with_defaults() {
        let args: ActArgs = serde_json::from_str(
            r#"{"steps":[{"do":"click","target":"e12"},{"do":"type","text":"hi","enter":true},{"do":"key","keys":"ctrl+s"}]}"#,
        )
        .unwrap();
        assert_eq!(args.after, AfterMode::Diff);
        assert!(args.stop_on_error);
        assert_eq!(
            args.steps[0],
            Step::Click {
                target: TargetSpec::Text("e12".to_string()),
                button: MouseButton::Left,
                count: 1,
                modifiers: None,
                how: ClickHow::Auto
            }
        );
        assert_eq!(args.steps[2], Step::Key { keys: "ctrl+s".to_string(), target: None, repeat: 1 });
    }

    #[test]
    fn wait_for_step_flattens_condition() {
        let step: Step = serde_json::from_str(r#"{"do":"wait_for","until":"appears","target":"name:Done"}"#).unwrap();
        let Step::WaitFor(condition) = step else { panic!("expected wait_for") };
        assert_eq!(condition.until, Until::Appears);
        assert_eq!(condition.timeout(), DEFAULT_WAIT);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<ObserveArgs>(r#"{"windw":"x"}"#).is_err());
        assert!(serde_json::from_str::<Condition>(r#"{"until":"idle","bogus":1}"#).is_err());
    }

    #[test]
    fn return_field_is_renamed() {
        let args: ActArgs = serde_json::from_str(r#"{"steps":[],"return":"none"}"#).unwrap();
        assert_eq!(args.after, AfterMode::None);
    }

    #[test]
    fn condition_validation_requires_fields() {
        let missing =
            Condition { until: Until::Text, target: None, text: None, window: None, ms: None, timeout_ms: None };
        assert_eq!(missing.validate().unwrap_err().code, crate::error::ErrorCode::InvalidArgument);
        let idle = Condition {
            until: Until::Idle,
            target: None,
            text: None,
            window: None,
            ms: Some(200),
            timeout_ms: Some(999_999),
        };
        assert_eq!(idle.validate().unwrap(), WaitCondition::Idle { quiet: Duration::from_millis(200) });
        assert_eq!(idle.timeout(), MAX_WAIT);
    }

    #[test]
    fn act_deadline_grows_with_steps_and_is_capped() {
        let wait = Step::Wait { ms: 1000 };
        let short = Call::Act(ActArgs { steps: vec![wait.clone()], after: AfterMode::Diff, stop_on_error: true });
        assert_eq!(short.deadline(), ACT_OVERHEAD + Duration::from_secs(2));
        let long = Call::Act(ActArgs { steps: vec![wait; 100], after: AfterMode::Diff, stop_on_error: true });
        assert_eq!(long.deadline(), MAX_DEADLINE);
    }

    #[test]
    fn read_only_classification() {
        assert!(Call::Status.is_read_only());
        let set = ClipboardArgs { action: ClipboardAction::Set, text: Some("x".to_string()) };
        assert!(!Call::Clipboard(set).is_read_only());
    }
}
