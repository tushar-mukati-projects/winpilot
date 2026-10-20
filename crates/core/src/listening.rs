use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    recording::RecordAudio,
};

pub(crate) const DEFAULT_LISTEN_SECONDS: u32 = 10;
const MAX_LISTEN_SECONDS: u32 = 90;
const LANGUAGE_CODE_LENGTHS: std::ops::RangeInclusive<usize> = 2..=3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenSource {
    System,
    Mic,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hearing {
    Live { source: ListenSource, seconds: u32 },
    File(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenRequest {
    pub hearing: Hearing,
    pub language: Option<String>,
    pub translate: bool,
    pub max_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeardSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenReport {
    pub heard: String,
    pub language: String,
    pub segments: Vec<HeardSegment>,
    pub truncated: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ListenSource {
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "system audio",
            Self::Mic => "the mic",
            Self::Both => "system audio and the mic",
        }
    }

    pub(crate) fn chosen(audio: Option<RecordAudio>) -> Result<Self> {
        match audio {
            None | Some(RecordAudio::System) => Ok(Self::System),
            Some(RecordAudio::Mic) => Ok(Self::Mic),
            Some(RecordAudio::Both) => Ok(Self::Both),
            Some(RecordAudio::None) => Err(Error::invalid("listen needs something to hear: audio system, mic or both")),
        }
    }
}

impl Hearing {
    pub(crate) fn chosen(path: Option<&str>, audio: Option<RecordAudio>, seconds: Option<u32>) -> Result<Self> {
        let Some(path) = path else { return live(audio, seconds) };
        if audio.is_some() || seconds.is_some() {
            return Err(Error::invalid(
                "listen with path transcribes that file; drop audio and max_seconds, which are for live listening",
            ));
        }
        let trimmed = path.trim();
        if !Path::new(trimmed).is_absolute() {
            return Err(Error::invalid(format!("path must be absolute, like C:\\Videos\\talk.mp4, not {trimmed:?}")));
        }
        Ok(Self::File(trimmed.to_string()))
    }
}

fn live(audio: Option<RecordAudio>, seconds: Option<u32>) -> Result<Hearing> {
    let source = ListenSource::chosen(audio)?;
    let seconds = seconds.unwrap_or(DEFAULT_LISTEN_SECONDS);
    if !(1..=MAX_LISTEN_SECONDS).contains(&seconds) {
        return Err(Error::invalid(format!(
            "max_seconds for listen must be 1-{MAX_LISTEN_SECONDS}; for longer, record with sound and then listen with its path"
        )));
    }
    Ok(Hearing::Live { source, seconds })
}

pub(crate) fn language_code(language: Option<&str>) -> Result<Option<String>> {
    let Some(language) = language.map(str::trim).filter(|language| !language.is_empty()) else { return Ok(None) };
    let code = language.to_ascii_lowercase();
    if code == "auto" {
        return Ok(None);
    }
    if LANGUAGE_CODE_LENGTHS.contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase()) {
        return Ok(Some(code));
    }
    Err(Error::invalid(format!("language takes a code like en or hi, or auto; {language:?} is not one")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_listening_defaults_to_ten_seconds_of_system_audio() {
        assert_eq!(
            Hearing::chosen(None, None, None).unwrap(),
            Hearing::Live { source: ListenSource::System, seconds: DEFAULT_LISTEN_SECONDS }
        );
        assert_eq!(
            Hearing::chosen(None, Some(RecordAudio::Both), Some(30)).unwrap(),
            Hearing::Live { source: ListenSource::Both, seconds: 30 }
        );
    }

    #[test]
    fn live_listening_has_a_source_and_a_bounded_length() {
        assert!(
            Hearing::chosen(None, Some(RecordAudio::None), None).unwrap_err().message.contains("something to hear")
        );
        assert!(Hearing::chosen(None, None, Some(0)).is_err());
        let long = Hearing::chosen(None, None, Some(MAX_LISTEN_SECONDS + 1)).unwrap_err();
        assert!(long.message.contains("record with sound"), "{}", long.message);
    }

    #[test]
    fn a_file_is_transcribed_by_its_absolute_path_alone() {
        assert_eq!(
            Hearing::chosen(Some(" C:\\Talks\\demo.mp4 "), None, None).unwrap(),
            Hearing::File("C:\\Talks\\demo.mp4".to_string())
        );
        assert!(Hearing::chosen(Some("demo.mp4"), None, None).unwrap_err().message.contains("absolute"));
        let mixed = Hearing::chosen(Some("C:\\a.mp4"), Some(RecordAudio::Mic), None).unwrap_err();
        assert!(mixed.message.contains("drop audio and max_seconds"), "{}", mixed.message);
        assert!(Hearing::chosen(Some("C:\\a.mp4"), None, Some(5)).is_err());
    }

    #[test]
    fn languages_are_short_codes_or_auto() {
        assert_eq!(language_code(None).unwrap(), None);
        assert_eq!(language_code(Some(" Auto ")).unwrap(), None);
        assert_eq!(language_code(Some("HI")).unwrap(), Some("hi".to_string()));
        assert_eq!(language_code(Some("yue")).unwrap(), Some("yue".to_string()));
        assert!(language_code(Some("hindi")).is_err());
        assert!(language_code(Some("e1")).is_err());
    }

    #[test]
    fn sources_read_naturally() {
        assert_eq!(ListenSource::System.name(), "system audio");
        assert_eq!(ListenSource::Both.name(), "system audio and the mic");
    }
}
