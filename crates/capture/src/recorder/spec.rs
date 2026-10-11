use std::path::PathBuf;

use deckhand_core::{
    Rect, WindowRef,
    protocol::{Quality, VideoFormat},
};

use crate::frame::Frame;

pub type Redactor = Box<dyn FnMut(&mut Frame) + Send>;
pub type StopCheck = Box<dyn Fn() -> bool + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingSource {
    Window { window: WindowRef, crop: Option<Rect> },
    Screen(Rect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSettings {
    pub format: VideoFormat,
    pub fps: u32,
    pub quality: Quality,
    pub max_seconds: u32,
    pub cursor: bool,
    pub max_width: u32,
}

pub struct RecordingSpec {
    pub source: RecordingSource,
    pub path: PathBuf,
    pub video: VideoSettings,
    pub redactor: Option<Redactor>,
    pub kill_switch: StopCheck,
}
