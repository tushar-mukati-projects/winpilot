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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordAudio {
    System,
    Mic,
    Both,
    None,
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
    pub audio: Option<RecordAudio>,
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
    #[serde(default)]
    pub audio: Option<RecordAudio>,
    pub note: Option<String>,
}

impl RecordAudio {
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Mic => "mic",
            Self::Both => "both",
            Self::None => "none",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "system audio",
            Self::Mic => "mic audio",
            Self::Both => "system and mic audio",
            Self::None => "no audio",
        }
    }

    pub(crate) fn for_format(asked: Option<Self>, format: VideoFormat) -> Result<Self> {
        match (format, asked) {
            (VideoFormat::Mp4, asked) => Ok(asked.unwrap_or(Self::System)),
            (VideoFormat::Gif, None | Some(Self::None)) => Ok(Self::None),
            (VideoFormat::Gif, Some(audio)) => {
                Err(Error::invalid(format!("gif recordings have no sound; drop audio={} or record mp4", audio.name())))
            }
        }
    }
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

    pub(crate) fn of_request(path: Option<&str>, asked: Option<Self>) -> Option<Self> {
        asked.or_else(|| path.and_then(Self::of_path))
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
            audio: None,
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
            ("audio", self.audio.is_some()),
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
        Self {
            state,
            path: None,
            seconds: 0.0,
            frames: 0,
            bytes: 0,
            width: 0,
            height: 0,
            fps: 0,
            audio: None,
            note: None,
        }
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
        args.audio = Some(RecordAudio::None);
        assert_eq!(args.start_options(), ["target", "fps", "path", "audio"]);
    }

    #[test]
    fn mp4_records_system_audio_unless_asked_otherwise() {
        assert_eq!(RecordAudio::for_format(None, VideoFormat::Mp4), Ok(RecordAudio::System));
        for audio in [RecordAudio::System, RecordAudio::Mic, RecordAudio::Both, RecordAudio::None] {
            assert_eq!(RecordAudio::for_format(Some(audio), VideoFormat::Mp4), Ok(audio));
        }
    }

    #[test]
    fn gif_is_always_silent_and_refuses_a_sound_source() {
        assert_eq!(RecordAudio::for_format(None, VideoFormat::Gif), Ok(RecordAudio::None));
        assert_eq!(RecordAudio::for_format(Some(RecordAudio::None), VideoFormat::Gif), Ok(RecordAudio::None));
        let error = RecordAudio::for_format(Some(RecordAudio::Mic), VideoFormat::Gif).unwrap_err();
        assert_eq!(error.message, "gif recordings have no sound; drop audio=mic or record mp4");
    }

    #[test]
    fn audio_sources_have_wire_names_and_labels() {
        assert_eq!(serde_json::to_value(RecordAudio::Both).unwrap(), serde_json::json!("both"));
        assert_eq!(serde_json::from_str::<RecordAudio>("\"none\"").unwrap(), RecordAudio::None);
        assert_eq!(RecordAudio::Mic.name(), "mic");
        assert_eq!(RecordAudio::Both.label(), "system and mic audio");
    }

    #[test]
    fn a_request_format_comes_from_format_or_the_path() {
        assert_eq!(VideoFormat::of_request(Some("C:\\v\\a.gif"), None), Some(VideoFormat::Gif));
        assert_eq!(VideoFormat::of_request(Some("C:\\v\\a.gif"), Some(VideoFormat::Mp4)), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::of_request(None, None), None);
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
