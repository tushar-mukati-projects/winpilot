mod sink;

use std::path::Path;

use windows::Win32::Media::MediaFoundation::{IMFMediaBuffer, IMFSinkWriter};
use winpilot_core::{Context, Error, Result, protocol::Quality};
use winpilot_win32::MediaFoundation;

use self::sink::{ByteStream, Sink, VideoSpec, open_sink, without_refused_audio};
use super::audio_backlog::TrackSamples;
use super::clock::sample_time;
use super::layout::Size;
use super::media_sample::{filled_buffer, write_sample};
use super::mp4_audio::write_audio;
use super::nv12::{bgra_to_nv12, nv12_len};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputPixels {
    Rgb32,
    Nv12,
}

const HD_HEIGHT: u32 = 720;

pub(crate) fn preferred_inputs(size: Size) -> Vec<InputPixels> {
    if size.height < HD_HEIGHT {
        return vec![InputPixels::Nv12, InputPixels::Rgb32];
    }
    vec![InputPixels::Rgb32, InputPixels::Nv12]
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AudioStream {
    None,
    Ready(u32),
    Refused(String),
}

pub(crate) struct Mp4Writer {
    writer: IMFSinkWriter,
    _file: ByteStream,
    stream: u32,
    audio: AudioStream,
    size: Size,
    fps: u32,
    input: InputPixels,
    shown: Option<IMFMediaBuffer>,
    next_tick: u64,
    _media_foundation: MediaFoundation,
}

impl Mp4Writer {
    pub(crate) fn create(
        path: &Path,
        size: Size,
        fps: u32,
        quality: Quality,
        inputs: &[InputPixels],
        with_audio: bool,
    ) -> Result<Self> {
        let media_foundation = MediaFoundation::start()?;
        let spec = VideoSpec { path, size, fps, quality, inputs };
        let Sink { writer, file, stream, input, audio } = without_refused_audio(open_sink(spec, with_audio)?, spec)?;
        unsafe { writer.BeginWriting() }
            .map_err(|error| Error::unsupported(format!("the H.264 encoder would not start: {error}")))?;
        Ok(Self {
            writer,
            _file: file,
            stream,
            audio,
            size,
            fps,
            input,
            shown: None,
            next_tick: 0,
            _media_foundation: media_foundation,
        })
    }

    pub(crate) fn write(&mut self, bgra: &[u8], tick: u64) -> Result<()> {
        let buffer = self.buffer_of(bgra)?;
        self.hold_until(tick)?;
        let next = tick.saturating_add(1);
        self.write_span(&buffer, tick, next)?;
        self.shown = Some(buffer);
        self.next_tick = next;
        Ok(())
    }

    pub(crate) fn repeat(&mut self, tick: u64) -> Result<()> {
        self.hold_until(tick.saturating_add(1))
    }

    pub(crate) fn audio_refusal(&self) -> Option<String> {
        match &self.audio {
            AudioStream::Refused(reason) => Some(reason.clone()),
            AudioStream::None | AudioStream::Ready(_) => None,
        }
    }

    pub(crate) fn write_audio(&self, samples: &TrackSamples) -> Result<()> {
        match self.audio {
            AudioStream::Ready(stream) => write_audio(&self.writer, stream, samples),
            AudioStream::None | AudioStream::Refused(_) => Ok(()),
        }
    }

    pub(crate) fn finish(mut self, end_tick: u64) -> Result<()> {
        self.hold_until(end_tick)?;
        unsafe { self.writer.Finalize() }.context("finalizing the MP4 file")
    }

    fn hold_until(&mut self, tick: u64) -> Result<()> {
        let Some(shown) = self.shown.clone() else { return Ok(()) };
        if tick <= self.next_tick {
            return Ok(());
        }
        self.write_span(&shown, self.next_tick, tick)?;
        self.next_tick = tick;
        Ok(())
    }

    fn write_span(&self, buffer: &IMFMediaBuffer, from_tick: u64, until_tick: u64) -> Result<()> {
        let (start, end) = (sample_time(from_tick, self.fps), sample_time(until_tick, self.fps));
        write_sample(&self.writer, self.stream, buffer, start, end)
    }

    fn buffer_of(&self, bgra: &[u8]) -> Result<IMFMediaBuffer> {
        match self.input {
            InputPixels::Rgb32 => filled_buffer(bgra.len(), |bytes| {
                bytes.copy_from_slice(bgra);
                Ok(())
            }),
            InputPixels::Nv12 => filled_buffer(nv12_len(self.size), |bytes| bgra_to_nv12(bgra, self.size, bytes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_videos_use_the_bt709_nv12_converter_and_hd_videos_send_rgb32() {
        assert_eq!(preferred_inputs(Size::new(640, 480)), [InputPixels::Nv12, InputPixels::Rgb32]);
        assert_eq!(preferred_inputs(Size::new(1920, 718)), [InputPixels::Nv12, InputPixels::Rgb32]);
        assert_eq!(preferred_inputs(Size::new(1280, 720)), [InputPixels::Rgb32, InputPixels::Nv12]);
        assert_eq!(preferred_inputs(Size::new(1920, 1200)), [InputPixels::Rgb32, InputPixels::Nv12]);
    }
}
