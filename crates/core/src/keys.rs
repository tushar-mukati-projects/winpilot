use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self { ctrl: false, alt: false, shift: false, win: false };

    pub const fn is_empty(self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.win)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Enter,
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    PrintScreen,
    Pause,
    CapsLock,
    NumLock,
    ScrollLock,
    ContextMenu,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    BrowserBack,
    BrowserForward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Char(char),
    Named(NamedKey),
    Function(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: Option<Key>,
}

pub fn parse_keys(spec: &str) -> Result<Vec<Chord>> {
    todo!("parse '{spec}'")
}

pub fn parse_chord(spec: &str) -> Result<Chord> {
    todo!("parse '{spec}'")
}
