use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    geometry::Rect,
    quality::Quality,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordAction {
    Start,
    Stop,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoFormat {
    Mp4,
    Gif,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRequest {
    pub action: RecordAction,
    pub target: Option<String>,
    pub region: Option<Rect>,
    pub monitor: Option<u32>,
    pub format: Option<VideoFormat>,
    pub fps: Option<u32>,
    pub quality: Option<Quality>,
    pub max_seconds: Option<u32>,
    pub cursor: Option<bool>,
    pub max_width: Option<u32>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    Idle,
    Starting,
    Recording,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordReport {
    pub state: RecordState,
    pub path: Option<String>,
    pub seconds: f64,
    pub frames: u64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub note: Option<String>,
}

impl VideoFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }

    pub(crate) const fn max_fps(self) -> u32 {
        match self {
            Self::Mp4 => 60,
            Self::Gif => 20,
        }
    }

    pub(crate) const fn default_max_width(self) -> u32 {
        match self {
            Self::Mp4 => 1920,
            Self::Gif => 960,
        }
    }

    fn of_path(path: &str) -> Option<Self> {
        let extension = Path::new(path.trim()).extension()?.to_str()?;
        [Self::Mp4, Self::Gif].into_iter().find(|format| format.extension().eq_ignore_ascii_case(extension))
    }

    pub(crate) fn for_path(path: &str, asked: Option<Self>) -> Result<Self> {
        let from_path = Self::of_path(path).ok_or_else(|| Error::invalid("path must end in .mp4 or .gif"))?;
        match asked {
            Some(format) if format != from_path => Err(Error::invalid(format!(
                "path ends in .{} but format is {}; make them match",
                from_path.extension(),
                format.extension()
            ))),
            _ => Ok(from_path),
        }
    }
}

impl RecordRequest {
    pub const fn new(action: RecordAction) -> Self {
        Self {
            action,
            target: None,
            region: None,
            monitor: None,
            format: None,
            fps: None,
            quality: None,
            max_seconds: None,
            cursor: None,
            max_width: None,
            path: None,
        }
    }

    pub(crate) fn start_options(&self) -> Vec<&'static str> {
        [
            ("target", self.target.is_some()),
            ("region", self.region.is_some()),
            ("monitor", self.monitor.is_some()),
            ("format", self.format.is_some()),
            ("fps", self.fps.is_some()),
            ("video_quality", self.quality.is_some()),
            ("max_seconds", self.max_seconds.is_some()),
            ("cursor", self.cursor.is_some()),
            ("max_width", self.max_width.is_some()),
            ("path", self.path.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, given)| given.then_some(name))
        .collect()
    }
}

impl RecordReport {
    pub const fn idle() -> Self {
        Self::empty(RecordState::Idle)
    }

    pub const fn starting() -> Self {
        Self::empty(RecordState::Starting)
    }

    const fn empty(state: RecordState) -> Self {
        Self { state, path: None, seconds: 0.0, frames: 0, bytes: 0, width: 0, height: 0, fps: 0, note: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_read_from_path_extensions_in_any_case() {
        assert_eq!(VideoFormat::of_path("C:\\v\\clip.MP4"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::of_path("C:\\v\\clip.gif "), Some(VideoFormat::Gif));
        assert_eq!(VideoFormat::of_path("C:\\v\\clip.avi"), None);
        assert_eq!(VideoFormat::of_path("C:\\v\\clip"), None);
        assert_eq!(VideoFormat::of_path("C:\\v\\.gif"), None);
    }

    #[test]
    fn start_options_lists_every_given_field_in_order() {
        let mut args = RecordRequest::new(RecordAction::Stop);
        assert!(args.start_options().is_empty());
        args.fps = Some(5);
        args.target = Some("w1".to_string());
        args.path = Some("C:\\a.mp4".to_string());
        assert_eq!(args.start_options(), ["target", "fps", "path"]);
    }

    #[test]
    fn idle_and_starting_reports_are_empty() {
        let idle = RecordReport::idle();
        assert_eq!((idle.state, idle.path, idle.frames, idle.bytes), (RecordState::Idle, None, 0, 0));
        let starting = RecordReport::starting();
        assert_eq!((starting.state, starting.path, starting.note), (RecordState::Starting, None, None));
        assert_eq!(serde_json::to_value(RecordState::Starting).unwrap(), serde_json::json!("starting"));
    }
}
