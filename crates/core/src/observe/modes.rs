use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    observe_request::ImageFormat,
    recording::VideoFormat,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObserveMode {
    #[default]
    Elements,
    Text,
    Screenshot,
    Record,
    Listen,
    Windows,
    Monitors,
    Apps,
    Clipboard,
    Status,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    #[default]
    Auto,
    Never,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaFormat {
    Jpeg,
    Png,
    Mp4,
    Gif,
}

impl ObserveMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Elements => "elements",
            Self::Text => "text",
            Self::Screenshot => "screenshot",
            Self::Record => "record",
            Self::Listen => "listen",
            Self::Windows => "windows",
            Self::Monitors => "monitors",
            Self::Apps => "apps",
            Self::Clipboard => "clipboard",
            Self::Status => "status",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Elements => "observe elements",
            Self::Text => "observe text",
            Self::Screenshot => "observe screenshot",
            Self::Record => "observe record",
            Self::Listen => "observe listen",
            Self::Windows => "observe windows",
            Self::Monitors => "observe monitors",
            Self::Apps => "observe apps",
            Self::Clipboard => "observe clipboard",
            Self::Status => "observe status",
        }
    }
}

impl MediaFormat {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }

    pub(super) fn image(self) -> Result<ImageFormat> {
        match self {
            Self::Jpeg => Ok(ImageFormat::Jpeg),
            Self::Png => Ok(ImageFormat::Png),
            Self::Mp4 | Self::Gif => {
                Err(Error::invalid(format!("format {} is for mode=record; screenshots take jpeg or png", self.name())))
            }
        }
    }

    pub(super) fn video(self) -> Result<VideoFormat> {
        match self {
            Self::Mp4 => Ok(VideoFormat::Mp4),
            Self::Gif => Ok(VideoFormat::Gif),
            Self::Jpeg | Self::Png => Err(Error::invalid(format!(
                "format {} is for mode=screenshot; recordings take mp4 or gif",
                self.name()
            ))),
        }
    }
}
