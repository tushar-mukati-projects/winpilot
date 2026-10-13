use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use winpilot_core::{
    Rect, WindowRef,
    protocol::{Quality, VideoFormat},
};

use super::output_file::OutputFile;
use crate::audio::AudioSource;
use crate::frame::Frame;

pub trait Redact: Send {
    fn refresh(&mut self) -> bool;
    fn cover(&self, frame: &mut Frame);
}

pub type Redactor = Box<dyn Redact>;
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
    pub audio: Option<AudioSource>,
}

pub struct RecordingSpec {
    pub source: RecordingSource,
    pub output: OutputFile,
    pub video: VideoSettings,
    pub redactor: Option<Redactor>,
    pub kill_switch: StopCheck,
    pub stop: Arc<AtomicBool>,
}
