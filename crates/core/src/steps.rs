use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    geometry::Rect,
    null_default::{or_default, or_one_u8, or_one_u32, or_true},
    target::TargetSpec,
    wait::WaitArgs,
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
    #[serde(deserialize_with = "numbered_steps")]
    #[schemars(description = "Up to 50 steps, run in order. Each is an object with a \"do\" field.", length(max = 50))]
    pub steps: Vec<Step>,
    #[serde(rename = "return", default, deserialize_with = "or_default")]
    #[schemars(description = "What to report afterwards: diff (default), observe, screenshot or none.")]
    pub after: AfterMode,
    #[serde(default = "default_true", deserialize_with = "or_true")]
    #[schemars(description = "Stop at the first failing step (default true).")]
    pub stop_on_error: bool,
    #[serde(default)]
    #[schemars(
        description = "Never touch the real mouse, keyboard or focus: UI Automation patterns, else window messages. Default: the [act] background config (false)."
    )]
    pub background: Option<bool>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    #[default]
    Press,
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MouseAction {
    Move,
    Down,
    Up,
    Scroll,
    Drag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    Launch,
    Close,
    Minimize,
    Maximize,
    Restore,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Click {
        target: TargetSpec,
        #[serde(default, deserialize_with = "or_default")]
        button: MouseButton,
        #[serde(default = "default_click_count", deserialize_with = "or_one_u8")]
        #[schemars(description = "Clicks: 1-3.", range(min = 1, max = 3))]
        count: u8,
        #[serde(default)]
        #[schemars(description = "Modifier keys to hold, e.g. ctrl or ctrl+shift.")]
        modifiers: Option<String>,
        #[serde(default, deserialize_with = "or_default")]
        #[schemars(description = "auto (a pattern on background windows, else the mouse), invoke or mouse.")]
        how: ClickHow,
        #[serde(default)]
        #[schemars(
            description = "Milliseconds the cursor glides first (default [cursor] glide_ms; 0 jumps; max 3000).",
            range(max = 3000)
        )]
        glide_ms: Option<u64>,
    },
    Type {
        #[serde(default)]
        #[schemars(
            description = "Field to type into. Omit to type into the focused app (never the agent's own terminal)."
        )]
        target: Option<TargetSpec>,
        #[schemars(description = "Text to type; newlines become Enter.")]
        text: String,
        #[serde(default, deserialize_with = "or_default")]
        #[schemars(description = "Press Enter after typing.")]
        enter: bool,
        #[serde(default, deserialize_with = "or_default")]
        #[schemars(description = "auto, keys or paste (the clipboard is restored).")]
        how: TypeHow,
    },
    Key {
        #[schemars(description = "Keys like ctrl+s, enter, alt+f4, or a sequence: ctrl+k ctrl+s.")]
        keys: String,
        #[serde(default)]
        #[schemars(
            description = "Element or window to send the keys to. Omit to use the focused app (never the agent's own terminal)."
        )]
        target: Option<TargetSpec>,
        #[serde(default = "default_one", deserialize_with = "or_one_u32")]
        #[schemars(description = "Times to press the keys: 1-100.", range(min = 1, max = 100))]
        repeat: u32,
        #[serde(default, deserialize_with = "or_default")]
        #[schemars(
            description = "press (default), down (hold) or up (release). Held keys are released when the call ends."
        )]
        action: KeyAction,
    },
    Mouse {
        #[schemars(description = "move, down, up, scroll or drag.")]
        action: MouseAction,
        #[serde(default)]
        #[schemars(
            description = "move: where to. drag: where from. down/up: default the cursor. scroll: default the window centre."
        )]
        target: Option<TargetSpec>,
        #[serde(default)]
        #[schemars(description = "drag: where to release.")]
        to: Option<TargetSpec>,
        #[serde(default)]
        #[schemars(description = "down, up, drag: left (default), right or middle.")]
        button: Option<MouseButton>,
        #[serde(default)]
        #[schemars(description = "scroll: up, down, left or right.")]
        direction: Option<Direction>,
        #[serde(default)]
        #[schemars(description = "scroll: wheel notches, 1-50 (default 3).", range(min = 1, max = 50))]
        amount: Option<u32>,
        #[serde(default)]
        #[schemars(
            description = "Milliseconds the cursor glides first (default [cursor] glide_ms; 0 jumps; max 3000).",
            range(max = 3000)
        )]
        glide_ms: Option<u64>,
    },
    Set {
        target: TargetSpec,
        #[serde(default)]
        #[schemars(description = "Text value. Give exactly one of value, option, checked, expanded or selected.")]
        value: Option<String>,
        #[serde(default)]
        #[schemars(description = "Item to choose in a combobox or list.")]
        option: Option<String>,
        #[serde(default)]
        #[schemars(description = "Check (true) or uncheck (false) a checkbox or toggle.")]
        checked: Option<bool>,
        #[serde(default)]
        #[schemars(description = "Expand (true) or collapse (false) a tree item, expander or menu.")]
        expanded: Option<bool>,
        #[serde(default)]
        #[schemars(description = "Select (true) or deselect (false) a list, tab or tree item.")]
        selected: Option<bool>,
    },
    Focus {
        target: TargetSpec,
    },
    Window {
        #[schemars(
            description = "launch (needs app), move (needs target and rect), or close/minimize/maximize/restore."
        )]
        action: WindowAction,
        #[serde(default)]
        #[schemars(description = "The window: a ref, title text or window:/process: selector.")]
        target: Option<TargetSpec>,
        #[serde(default)]
        #[schemars(description = "launch: a Start-menu name like Notepad, an app id or an exe path.")]
        app: Option<String>,
        #[serde(default)]
        #[schemars(description = "launch: command-line arguments.")]
        args: Option<String>,
        #[serde(default)]
        #[schemars(description = "move: the new screen rectangle.")]
        rect: Option<Rect>,
    },
    Pick {
        #[schemars(description = "Absolute file paths. Several files must share one folder.")]
        paths: Vec<String>,
        #[serde(default)]
        #[schemars(description = "Click this first to open the dialog, e.g. an Upload button.")]
        open_with: Option<TargetSpec>,
        #[serde(default, deserialize_with = "or_default")]
        #[schemars(description = "In a Save dialog, confirm replacing an existing file.")]
        overwrite: bool,
        #[serde(default)]
        #[schemars(
            description = "How long open_with may take to open the dialog (default 15000, max 60000).",
            range(max = 60_000)
        )]
        timeout_ms: Option<u64>,
    },
    Wait(WaitArgs),
    Clipboard {
        #[schemars(description = "Text to put on the clipboard.")]
        text: String,
    },
    Notify {
        #[schemars(description = "Bold first line, 1-63 characters.", length(min = 1, max = 63))]
        title: String,
        #[schemars(description = "Body text, 1-255 characters.", length(min = 1, max = 255))]
        message: String,
    },
}

fn numbered_steps<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Step>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    raw.into_iter()
        .enumerate()
        .map(|(index, step)| {
            serde_json::from_value(step).map_err(|error| de::Error::custom(format!("step {}: {error}", index + 1)))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetChange {
    Value(String),
    Option(String),
    Checked(bool),
    Expanded(bool),
    Selected(bool),
}

impl Step {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Click { .. } => "click",
            Self::Type { .. } => "type",
            Self::Key { .. } => "key",
            Self::Mouse { .. } => "mouse",
            Self::Set { .. } => "set",
            Self::Focus { .. } => "focus",
            Self::Window { .. } => "window",
            Self::Pick { .. } => "pick",
            Self::Wait(_) => "wait",
            Self::Clipboard { .. } => "clipboard",
            Self::Notify { .. } => "notify",
        }
    }

    pub const fn uses_input(&self) -> bool {
        !matches!(self, Self::Wait(_) | Self::Notify { .. })
    }
}

impl MouseAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Down => "down",
            Self::Up => "up",
            Self::Scroll => "scroll",
            Self::Drag => "drag",
        }
    }
}

impl KeyAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

impl WindowAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Close => "close",
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Restore => "restore",
            Self::Move => "move",
        }
    }
}

impl MouseButton {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

impl Direction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{target::PointSpec, wait::Until};

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn step_error(json: &str) -> String {
        serde_json::from_str::<Step>(json).unwrap_err().to_string()
    }

    fn text(target: &str) -> TargetSpec {
        TargetSpec::Text(target.to_string())
    }

    #[test]
    fn steps_deserialize_with_defaults() {
        let args: ActArgs = serde_json::from_str(
            r#"{"steps":[{"do":"click","target":"e12"},{"do":"type","text":"hi","enter":true},{"do":"key","keys":"ctrl+s"}]}"#,
        )
        .unwrap();
        assert_eq!((args.after, args.stop_on_error, args.background), (AfterMode::Diff, true, None));
        assert_eq!(
            args.steps[0],
            Step::Click {
                target: text("e12"),
                button: MouseButton::Left,
                count: 1,
                modifiers: None,
                how: ClickHow::Auto,
                glide_ms: None,
            }
        );
        assert_eq!(
            args.steps[2],
            Step::Key { keys: "ctrl+s".to_string(), target: None, repeat: 1, action: KeyAction::Press }
        );
    }

    #[test]
    fn return_and_background_parse() {
        let args: ActArgs = serde_json::from_str(r#"{"steps":[],"return":"none","background":true}"#).unwrap();
        assert_eq!((args.after, args.background), (AfterMode::None, Some(true)));
    }

    #[test]
    fn mouse_steps_carry_an_action() {
        assert_eq!(
            step(r#"{"do":"mouse","action":"move","target":{"x":10,"y":20},"glide_ms":300}"#),
            Step::Mouse {
                action: MouseAction::Move,
                target: Some(TargetSpec::Point(PointSpec { x: 10, y: 20, img: None })),
                to: None,
                button: None,
                direction: None,
                amount: None,
                glide_ms: Some(300),
            }
        );
        let Step::Mouse { action, direction, .. } = step(r#"{"do":"mouse","action":"scroll","direction":"down"}"#)
        else {
            panic!("expected mouse")
        };
        assert_eq!((action, direction), (MouseAction::Scroll, Some(Direction::Down)));
        assert!(step_error(r#"{"do":"mouse","target":"e1"}"#).contains("missing field `action`"));
    }

    #[test]
    fn set_window_pick_clipboard_and_notify_parse() {
        let Step::Set { checked, value, .. } = step(r#"{"do":"set","target":"e5","checked":true}"#) else {
            panic!("expected set")
        };
        assert_eq!((checked, value), (Some(true), None));
        let Step::Window { action, app, .. } = step(r#"{"do":"window","action":"launch","app":"Notepad"}"#) else {
            panic!("expected window")
        };
        assert_eq!((action, app.as_deref()), (WindowAction::Launch, Some("Notepad")));
        let Step::Pick { paths, overwrite, .. } = step(r#"{"do":"pick","paths":["C:\\a.txt"]}"#) else {
            panic!("expected pick")
        };
        assert_eq!((paths.len(), overwrite), (1, false));
        assert_eq!(step(r#"{"do":"clipboard","text":"x"}"#), Step::Clipboard { text: "x".to_string() });
        assert!(matches!(step(r#"{"do":"notify","title":"Done","message":"m"}"#), Step::Notify { .. }));
    }

    #[test]
    fn wait_steps_flatten_their_fields() {
        let Step::Wait(wait) = step(r#"{"do":"wait","until":"appears","target":"name:Done"}"#) else {
            panic!("expected wait")
        };
        assert_eq!((wait.until, wait.target.as_deref()), (Some(Until::Appears), Some("name:Done")));
        let Step::Wait(pause) = step(r#"{"do":"wait","ms":10}"#) else { panic!("expected wait") };
        assert_eq!((pause.until, pause.ms), (None, Some(10)));
    }

    #[test]
    fn misspelled_and_retired_fields_and_steps_are_rejected() {
        let cases = [
            (r#"{"do":"click","target":"e1","modifier":"ctrl"}"#, "unknown field `modifier`"),
            (r#"{"do":"type","text":"hi","press_enter":true}"#, "unknown field `press_enter`"),
            (r#"{"do":"click","target":"e1","buton":"right"}"#, "unknown field `buton`"),
            (r#"{"do":"wait","until":"idle","bogus":1}"#, "unknown field `bogus`"),
            (r#"{"do":"mouse","action":"drag","from":"e1","to":"e2"}"#, "unknown field `from`"),
            (r#"{"do":"set_value","target":"e1","value":"x"}"#, "unknown variant `set_value`"),
            (r#"{"do":"hover","target":"e1"}"#, "unknown variant `hover`"),
            (r#"{"do":"wait_for","until":"idle"}"#, "unknown variant `wait_for`"),
        ];
        for (json, expected) in cases {
            let message = step_error(json);
            assert!(message.contains(expected), "{json}: {message}");
        }
        assert!(serde_json::from_str::<Step>(r#"{"do":"click","target":{"x":1,"y":2,"image":"i3"}}"#).is_err());
    }

    #[test]
    fn a_step_that_does_not_parse_is_named_by_its_number() {
        let error = serde_json::from_str::<ActArgs>(r#"{"steps":[{"do":"wait","ms":1},{"do":"clik","target":"e1"}]}"#)
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("step 2: unknown variant `clik`"), "{error}");
    }

    #[test]
    fn the_schema_states_the_real_limits() {
        let schema = serde_json::to_value(schemars::schema_for!(ActArgs)).unwrap();
        assert_eq!(schema["properties"]["steps"]["maxItems"], 50);
        let text = schema.to_string();
        for limit in
            [r#""maximum":3"#, r#""maximum":100"#, r#""maximum":3000"#, r#""maxLength":63"#, r#""maxLength":255"#]
        {
            assert!(text.contains(limit), "missing {limit}");
        }
    }

    #[test]
    fn null_fields_take_their_defaults() {
        let args: ActArgs = serde_json::from_str(
            r#"{"steps":[{"do":"click","target":"e1","button":null,"count":null,"how":null},
            {"do":"type","text":"x","enter":null,"how":null},{"do":"key","keys":"a","repeat":null,"action":null},
            {"do":"pick","paths":["C:\\a.txt"],"overwrite":null}],"return":null,"stop_on_error":null}"#,
        )
        .unwrap();
        assert_eq!(args.after, AfterMode::Diff);
        assert!(args.stop_on_error);
        assert!(matches!(args.steps[0], Step::Click { button: MouseButton::Left, count: 1, how: ClickHow::Auto, .. }));
        assert!(matches!(args.steps[1], Step::Type { enter: false, how: TypeHow::Auto, .. }));
        assert!(matches!(args.steps[2], Step::Key { repeat: 1, action: KeyAction::Press, .. }));
        assert!(matches!(args.steps[3], Step::Pick { overwrite: false, .. }));
    }

    #[test]
    fn kinds_and_input_use() {
        assert_eq!(step(r#"{"do":"focus","target":"w1"}"#).kind(), "focus");
        assert!(step(r#"{"do":"clipboard","text":"x"}"#).uses_input());
        assert!(!step(r#"{"do":"wait","ms":1}"#).uses_input());
        assert!(!step(r#"{"do":"notify","title":"a","message":"b"}"#).uses_input());
    }

    #[test]
    fn names_are_the_wire_names() {
        for action in [MouseAction::Move, MouseAction::Down, MouseAction::Up, MouseAction::Scroll, MouseAction::Drag] {
            assert_eq!(serde_json::to_value(action).unwrap(), serde_json::json!(action.name()));
        }
        for action in [WindowAction::Launch, WindowAction::Close, WindowAction::Move] {
            assert_eq!(serde_json::to_value(action).unwrap(), serde_json::json!(action.name()));
        }
        for action in [KeyAction::Press, KeyAction::Down, KeyAction::Up] {
            assert_eq!(serde_json::to_value(action).unwrap(), serde_json::json!(action.name()));
        }
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            assert_eq!(serde_json::to_value(button).unwrap(), serde_json::json!(button.name()));
        }
        for direction in [Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
            assert_eq!(serde_json::to_value(direction).unwrap(), serde_json::json!(direction.name()));
        }
    }
}
