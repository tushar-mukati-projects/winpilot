use std::path::PathBuf;

use deckhand_core::{
    Result,
    protocol::{Quality, VideoFormat},
};

use super::audio_backlog::TrackSamples;
use super::gif_writer::GifWriter;
use super::layout::Size;
use super::mp4::{InputPixels, Mp4Writer, preferred_inputs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WriterSettings {
    pub path: PathBuf,
    pub format: VideoFormat,
    pub size: Size,
    pub fps: u32,
    pub quality: Quality,
    pub mp4_inputs: Vec<InputPixels>,
    pub audio: bool,
}

pub(crate) enum VideoWriter {
    Mp4(Mp4Writer),
    Gif(GifWriter),
}

impl WriterSettings {
    pub(crate) fn new(path: PathBuf, format: VideoFormat, size: Size, fps: u32, quality: Quality) -> Self {
        Self { path, format, size, fps, quality, mp4_inputs: preferred_inputs(size), audio: false }
    }
}

impl VideoWriter {
    pub(crate) fn create(settings: &WriterSettings) -> Result<Self> {
        let WriterSettings { path, format, size, fps, quality, mp4_inputs, audio } = settings;
        match format {
            VideoFormat::Mp4 => Mp4Writer::create(path, *size, *fps, *quality, mp4_inputs, *audio).map(Self::Mp4),
            VideoFormat::Gif => GifWriter::create(path, *size, *fps, *quality).map(Self::Gif),
        }
    }

    pub(crate) fn write(&mut self, bgra: &[u8], tick: u64) -> Result<()> {
        match self {
            Self::Mp4(writer) => writer.write(bgra, tick),
            Self::Gif(writer) => writer.write(bgra, tick),
        }
    }

    pub(crate) fn repeat(&mut self, tick: u64) -> Result<()> {
        match self {
            Self::Mp4(writer) => writer.repeat(tick),
            Self::Gif(_) => Ok(()),
        }
    }

    pub(crate) fn write_audio(&self, samples: &TrackSamples) -> Result<()> {
        match self {
            Self::Mp4(writer) => writer.write_audio(samples),
            Self::Gif(_) => Ok(()),
        }
    }

    pub(crate) fn audio_refusal(&self) -> Option<String> {
        match self {
            Self::Mp4(writer) => writer.audio_refusal(),
            Self::Gif(_) => None,
        }
    }

    pub(crate) fn finish(self, end_tick: u64) -> Result<()> {
        match self {
            Self::Mp4(writer) => writer.finish(end_tick),
            Self::Gif(writer) => writer.finish(end_tick),
        }
    }
}
