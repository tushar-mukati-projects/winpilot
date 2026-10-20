use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
