use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    diff::Change,
    element::{Element, Snapshot, WindowInfo},
    error::{Error, Result},
    geometry::{ImageMapping, Rect},
    keys::parse_keys,
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
    #[schemars(description = "Image format: jpeg (default) or png for lossless crops of small text.")]
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
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
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
        #[schemars(description = "Milliseconds to pause (at most 60000), capped at 5 s when run.")]
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
    #[schemars(description = "Element ref or selector to watch, for appears/disappears.")]
    pub target: Option<String>,
    #[serde(default)]
    #[schemars(description = "Text to wait for, for until=text.")]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(description = "Window title text for window/window_closed, or the window to watch for the others.")]
    pub window: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "How long the UI must stay quiet, for until=idle (default 300, must be shorter than timeout_ms)."
    )]
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

pub const NOTIFY_TITLE_MAX: usize = 63;
pub const NOTIFY_MESSAGE_MAX: usize = 255;
pub const DEFAULT_WAIT: Duration = Duration::from_secs(5);
pub const MAX_WAIT: Duration = Duration::from_secs(60);
pub const DEFAULT_IDLE: Duration = Duration::from_millis(300);
pub const MAX_PLAIN_WAIT: Duration = Duration::from_secs(5);

impl Condition {
    pub fn timeout(&self) -> Duration {
        self.timeout_ms.map_or(DEFAULT_WAIT, Duration::from_millis).min(MAX_WAIT)
    }

    pub fn validate(&self) -> Result<WaitCondition> {
        match self.until {
            Until::Appears => self.appears(),
            Until::Disappears => self.disappears(),
            Until::Window => self.window_opens(),
            Until::WindowClosed => self.window_closes(),
            Until::Text => self.text_appears(),
            Until::Idle => self.idle(),
        }
    }

    fn appears(&self) -> Result<WaitCondition> {
        Ok(WaitCondition::Appears { target: self.required_target()?, window: non_blank(self.window.as_ref()) })
    }

    fn disappears(&self) -> Result<WaitCondition> {
        Ok(WaitCondition::Disappears { target: self.required_target()?, window: non_blank(self.window.as_ref()) })
    }

    fn window_opens(&self) -> Result<WaitCondition> {
        Ok(WaitCondition::WindowOpens { title: self.required(self.window.as_ref(), "window")? })
    }

    fn window_closes(&self) -> Result<WaitCondition> {
        Ok(WaitCondition::WindowCloses { title: self.required(self.window.as_ref(), "window")? })
    }

    fn text_appears(&self) -> Result<WaitCondition> {
        let text = self.required(self.text.as_ref(), "text")?;
        Ok(WaitCondition::Text { text, window: non_blank(self.window.as_ref()) })
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

    fn required_target(&self) -> Result<Target> {
        parse_text_target(&self.required(self.target.as_ref(), "target")?)
    }

    fn required(&self, value: Option<&String>, field: &str) -> Result<String> {
        non_blank(value).ok_or_else(|| Error::invalid(format!("until={} needs \"{field}\"", until_name(self.until))))
    }
}

fn non_blank(value: Option<&String>) -> Option<String> {
    value.map(|text| text.trim()).filter(|text| !text.is_empty()).map(str::to_string)
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
    Monitors,
    Apps,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowsArgs {
    pub action: WindowAction,
    #[serde(default)]
    #[schemars(description = "Window ref or selector for focus/close/minimize/maximize/restore/move.")]
    pub window: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "App to launch: a Start-menu name like Notepad, an app id, or an exe path. For apps: a name filter."
    )]
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
    #[schemars(description = "auto (default: UI Automation text first, OCR as fallback), uia, or ocr.")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotifyArgs {
    #[schemars(description = "Bold first line, 1 to 63 characters.")]
    pub title: String,
    #[schemars(description = "Body text, 1 to 255 characters.")]
    pub message: String,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    Packaged,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub id: String,
    pub kind: AppKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub index: u32,
    pub rect: Rect,
    pub work_area: Rect,
    pub scale_percent: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsReport {
    pub windows: Vec<WindowInfo>,
    pub window: Option<WindowInfo>,
    pub observation: Option<Observation>,
    pub note: Option<String>,
    #[serde(default)]
    pub monitors: Vec<MonitorInfo>,
    #[serde(default)]
    pub apps: Vec<AppInfo>,
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
pub struct NotifyReport {
    pub shown: bool,
    pub note: Option<String>,
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
    Notify(NotifyArgs),
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
    Notify(NotifyReport),
    Status(StatusReport),
}

const STEP_BUDGET: Duration = Duration::from_secs(6);
const ACT_OVERHEAD: Duration = Duration::from_secs(10);
const MAX_DEADLINE: Duration = Duration::from_secs(180);
const DEFAULT_PICK: Duration = Duration::from_secs(15);
const CHARS_PER_EXTRA_SECOND: u64 = 2000;
const KEY_PRESS_BUDGET: Duration = Duration::from_millis(100);

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
            Self::Notify(_) => "notify",
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
            Self::Windows(args) => {
                matches!(args.action, WindowAction::List | WindowAction::Monitors | WindowAction::Apps)
            }
            Self::Clipboard(args) => args.action == ClipboardAction::Get,
            Self::Act(_) | Self::PickFile(_) | Self::Notify(_) => false,
        }
    }

    pub fn deadline(&self) -> Duration {
        let deadline = match self {
            Self::Windows(WindowsArgs { action: WindowAction::Launch, .. }) => Duration::from_secs(30),
            Self::Observe(_) | Self::Find(_) | Self::Windows(_) => Duration::from_secs(10),
            Self::Screenshot(_) => Duration::from_secs(8),
            Self::ReadText(_) => Duration::from_secs(20),
            Self::Status => Duration::from_secs(3),
            Self::Clipboard(_) | Self::Notify(_) => Duration::from_secs(5),
            Self::WaitFor(condition) => condition.timeout() + Duration::from_secs(5),
            Self::PickFile(args) => {
                args.timeout_ms.map_or(DEFAULT_PICK, Duration::from_millis).min(MAX_WAIT) + ACT_OVERHEAD
            }
            Self::Act(args) => args.steps.iter().map(step_budget).fold(ACT_OVERHEAD, Duration::saturating_add),
        };
        deadline.min(MAX_DEADLINE)
    }
}

fn step_budget(step: &Step) -> Duration {
    match step {
        Step::WaitFor(condition) => condition.timeout() + Duration::from_secs(1),
        Step::Wait { ms } => Duration::from_millis(*ms).min(MAX_PLAIN_WAIT) + Duration::from_secs(1),
        Step::Type { text, .. } => STEP_BUDGET.saturating_add(typing_time(text)),
        Step::SetValue { value, .. } => STEP_BUDGET.saturating_add(typing_time(value)),
        Step::Key { keys, repeat, .. } => STEP_BUDGET.saturating_add(key_time(keys, *repeat)),
        _ => STEP_BUDGET,
    }
}

fn typing_time(text: &str) -> Duration {
    let chars = text.chars().count() as u64;
    Duration::from_secs(chars.div_ceil(CHARS_PER_EXTRA_SECOND))
}

fn key_time(keys: &str, repeat: u32) -> Duration {
    let chords = parse_keys(keys).map_or(1, |chords| chords.len());
    let presses = u32::try_from(chords).unwrap_or(u32::MAX).saturating_mul(repeat);
    KEY_PRESS_BUDGET.saturating_mul(presses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

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
        assert_eq!(missing.validate().unwrap_err().code, ErrorCode::InvalidArgument);
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

    fn windows_call(action: WindowAction) -> Call {
        Call::Windows(WindowsArgs { action, window: None, app: None, args: None, rect: None })
    }

    fn notify_call() -> Call {
        Call::Notify(NotifyArgs { title: "Export".to_string(), message: "Export finished".to_string() })
    }

    #[test]
    fn monitors_are_read_only_and_notify_is_not() {
        assert!(windows_call(WindowAction::Monitors).is_read_only());
        assert!(windows_call(WindowAction::Apps).is_read_only());
        assert!(!windows_call(WindowAction::Launch).is_read_only());
        assert!(windows_call(WindowAction::List).is_read_only());
        assert!(!windows_call(WindowAction::Close).is_read_only());
        assert!(!notify_call().is_read_only());
    }

    #[test]
    fn notify_and_monitors_have_names_and_deadlines() {
        assert_eq!(notify_call().name(), "notify");
        assert_eq!(notify_call().deadline(), Duration::from_secs(5));
        assert_eq!(windows_call(WindowAction::Monitors).name(), "windows");
        assert_eq!(windows_call(WindowAction::Monitors).deadline(), Duration::from_secs(10));
        assert_eq!(windows_call(WindowAction::Apps).deadline(), Duration::from_secs(10));
    }

    #[test]
    fn monitors_action_and_notify_args_parse() {
        let args: WindowsArgs = serde_json::from_str(r#"{"action":"monitors"}"#).unwrap();
        assert_eq!(args.action, WindowAction::Monitors);
        let notify: NotifyArgs = serde_json::from_str(r#"{"title":"Done","message":"Export finished"}"#).unwrap();
        assert_eq!(notify.title, "Done");
        assert!(serde_json::from_str::<NotifyArgs>(r#"{"title":"Done","message":"m","icon":"x"}"#).is_err());
        assert!(serde_json::from_str::<NotifyArgs>(r#"{"title":"Done"}"#).is_err());
    }

    #[test]
    fn windows_reports_without_monitors_still_deserialize() {
        let report: WindowsReport =
            serde_json::from_str(r#"{"windows":[],"window":null,"observation":null,"note":"closed w5"}"#).unwrap();
        assert!(report.monitors.is_empty());
        assert!(report.apps.is_empty());
        assert_eq!(report.note.as_deref(), Some("closed w5"));
    }

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn step_error(json: &str) -> String {
        serde_json::from_str::<Step>(json).unwrap_err().to_string()
    }

    fn condition(json: &str) -> Condition {
        serde_json::from_str(json).unwrap()
    }

    fn invalid_message(result: Result<impl std::fmt::Debug>) -> String {
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    #[test]
    fn misspelled_step_fields_are_rejected() {
        let cases = [
            (r#"{"do":"click","target":"e1","modifier":"ctrl"}"#, "modifier"),
            (r#"{"do":"type","text":"hi","press_enter":true}"#, "press_enter"),
            (r#"{"do":"type","into":"e3","text":"hi"}"#, "into"),
            (r#"{"do":"click","target":"e1","buton":"right"}"#, "buton"),
            (r#"{"do":"wait_for","until":"idle","bogus":1}"#, "bogus"),
        ];
        for (json, field) in cases {
            let message = step_error(json);
            assert!(message.contains(&format!("unknown field `{field}`")), "{json}: {message}");
        }
        assert!(serde_json::from_str::<Step>(r#"{"do":"click","target":{"x":1,"y":2,"image":"i3"}}"#).is_err());
    }

    #[test]
    fn correctly_spelled_steps_still_parse() {
        assert_eq!(step(r#"{"do":"wait","ms":10}"#), Step::Wait { ms: 10 });
        let Step::WaitFor(wait) = step(r#"{"do":"wait_for","until":"window","window":"Save As"}"#) else {
            panic!("expected wait_for")
        };
        assert_eq!(wait.window.as_deref(), Some("Save As"));
        let Step::Click { target, .. } = step(r#"{"do":"click","target":{"x":1,"y":2,"img":"i3"}}"#) else {
            panic!("expected click")
        };
        assert_eq!(target, TargetSpec::Point(crate::target::PointSpec { x: 1, y: 2, img: Some("i3".to_string()) }));
    }

    #[test]
    fn blank_condition_strings_count_as_missing() {
        let appears = condition(r#"{"until":"appears","target":"e1","window":"  "}"#);
        assert_eq!(
            appears.validate().unwrap(),
            WaitCondition::Appears { target: Target::Element(crate::refs::ElementRef(1)), window: None }
        );
        let text = condition(r#"{"until":"text","text":"Done","window":""}"#);
        assert_eq!(text.validate().unwrap(), WaitCondition::Text { text: "Done".to_string(), window: None });
        let watched = condition(r#"{"until":"disappears","target":"e2","window":" Notepad "}"#);
        assert!(
            matches!(watched.validate().unwrap(), WaitCondition::Disappears { window: Some(title), .. } if title == "Notepad")
        );
        let blank_target = condition(r#"{"until":"appears","target":"   "}"#);
        assert_eq!(invalid_message(blank_target.validate()), "until=appears needs \"target\"");
        let blank_window = condition(r#"{"until":"window_closed","window":" "}"#);
        assert_eq!(invalid_message(blank_window.validate()), "until=window_closed needs \"window\"");
    }

    #[test]
    fn idle_quiet_time_must_be_shorter_than_the_timeout() {
        let long = condition(r#"{"until":"idle","ms":30000}"#);
        assert_eq!(
            invalid_message(long.validate()),
            "idle ms must be shorter than timeout_ms (quiet 30000 ms, timeout 5000 ms)"
        );
        let equal = condition(r#"{"until":"idle","ms":2000,"timeout_ms":2000}"#);
        assert!(equal.validate().is_err());
        let default_quiet = condition(r#"{"until":"idle","timeout_ms":200}"#);
        assert!(default_quiet.validate().is_err());
        let fine = condition(r#"{"until":"idle","ms":1999,"timeout_ms":2000}"#);
        assert_eq!(fine.validate().unwrap(), WaitCondition::Idle { quiet: Duration::from_millis(1999) });
        assert_eq!(condition(r#"{"until":"idle"}"#).validate().unwrap(), WaitCondition::Idle { quiet: DEFAULT_IDLE });
    }

    #[test]
    fn act_deadline_accounts_for_typing_and_key_repeats() {
        let act = |step: Step| Call::Act(ActArgs { steps: vec![step], after: AfterMode::Diff, stop_on_error: true });
        let short = Step::Type { target: None, text: "hi".to_string(), enter: false, how: TypeHow::Auto };
        assert_eq!(act(short).deadline(), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(1));
        let long = Step::Type { target: None, text: "a".repeat(10_000), enter: false, how: TypeHow::Auto };
        assert_eq!(act(long).deadline(), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(5));
        let empty = Step::Type { target: None, text: String::new(), enter: false, how: TypeHow::Auto };
        assert_eq!(act(empty).deadline(), ACT_OVERHEAD + STEP_BUDGET);
        let keys = Step::Key { keys: "ctrl+k ctrl+s".to_string(), target: None, repeat: 50 };
        assert_eq!(act(keys).deadline(), ACT_OVERHEAD + STEP_BUDGET + Duration::from_secs(10));
        let unparsable = Step::Key { keys: "bogus".to_string(), target: None, repeat: u32::MAX };
        assert_eq!(act(unparsable).deadline(), MAX_DEADLINE);
    }
}
