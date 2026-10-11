use std::path::Path;

use deckhand_core::{Context, Error, Result, protocol::Quality};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFByteStream, IMFMediaBuffer, IMFSinkWriter, MF_ACCESSMODE_READWRITE, MF_FILEFLAGS_NONE,
    MF_OPENMODE_FAIL_IF_NOT_EXIST, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_TRANSCODE_CONTAINERTYPE,
    MFCreateAttributes, MFCreateFile, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFTranscodeContainerType_FMPEG4,
};
use windows_core::HSTRING;

use super::bitrate::average_bitrate;
use super::clock::sample_time;
use super::layout::Size;
use super::media_foundation::MediaFoundation;
use super::media_type::{h264_output, nv12_input, rgb32_input};
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

pub(crate) struct Mp4Writer {
    writer: IMFSinkWriter,
    _file: ByteStream,
    stream: u32,
    size: Size,
    fps: u32,
    input: InputPixels,
    shown: Option<IMFMediaBuffer>,
    next_tick: u64,
    _media_foundation: MediaFoundation,
}

struct ByteStream(IMFByteStream);

impl Mp4Writer {
    pub(crate) fn create(path: &Path, size: Size, fps: u32, quality: Quality, inputs: &[InputPixels]) -> Result<Self> {
        let media_foundation = MediaFoundation::start()?;
        let file = ByteStream::open_existing(path)?;
        let writer = sink_writer(path, &file)?;
        let output = h264_output(size, fps, average_bitrate(size.width, size.height, fps, quality))?;
        let stream = unsafe { writer.AddStream(&output) }
            .map_err(|error| Error::unsupported(format!("no H.264 encoder is available: {error}")))?;
        let input = accepted_input(&writer, stream, size, fps, inputs)?;
        unsafe { writer.BeginWriting() }
            .map_err(|error| Error::unsupported(format!("the H.264 encoder would not start: {error}")))?;
        Ok(Self {
            writer,
            _file: file,
            stream,
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
        let start = sample_time(from_tick, self.fps);
        let sample = unsafe { MFCreateSample() }.context("creating a video sample")?;
        unsafe { sample.AddBuffer(buffer) }.context("filling a video sample")?;
        unsafe { sample.SetSampleTime(start) }.context("timing a video frame")?;
        unsafe { sample.SetSampleDuration(sample_time(until_tick, self.fps) - start) }
            .context("timing a video frame")?;
        unsafe { self.writer.WriteSample(self.stream, &sample) }.context("encoding a video frame")
    }

    fn buffer_of(&self, bgra: &[u8]) -> Result<IMFMediaBuffer> {
        let length = match self.input {
            InputPixels::Rgb32 => bgra.len(),
            InputPixels::Nv12 => nv12_len(self.size),
        };
        let byte_count = u32::try_from(length).map_err(|_| Error::internal("the video frame is too large"))?;
        let buffer = unsafe { MFCreateMemoryBuffer(byte_count) }.context("allocating a video frame")?;
        self.fill(&buffer, bgra, length)?;
        unsafe { buffer.SetCurrentLength(byte_count) }.context("sizing a video frame")?;
        Ok(buffer)
    }

    fn fill(&self, buffer: &IMFMediaBuffer, bgra: &[u8], length: usize) -> Result<()> {
        let mut locked = LockedBuffer::lock(buffer, length)?;
        match self.input {
            InputPixels::Rgb32 => {
                locked.bytes_mut().copy_from_slice(bgra);
                Ok(())
            }
            InputPixels::Nv12 => bgra_to_nv12(bgra, self.size, locked.bytes_mut()),
        }
    }
}

impl ByteStream {
    fn open_existing(path: &Path) -> Result<Self> {
        let url = HSTRING::from(path.as_os_str());
        let stream =
            unsafe { MFCreateFile(MF_ACCESSMODE_READWRITE, MF_OPENMODE_FAIL_IF_NOT_EXIST, MF_FILEFLAGS_NONE, &url) }
                .context("opening the reserved MP4 file")?;
        Ok(Self(stream))
    }
}

impl Drop for ByteStream {
    fn drop(&mut self) {
        let _ = unsafe { self.0.Close() };
    }
}

fn sink_writer(path: &Path, file: &ByteStream) -> Result<IMFSinkWriter> {
    let mut attributes: Option<IMFAttributes> = None;
    unsafe { MFCreateAttributes(&raw mut attributes, 2) }.context("creating the MP4 writer settings")?;
    let attributes = attributes.ok_or_else(|| Error::internal("Media Foundation returned no attribute store"))?;
    unsafe { attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1) }
        .context("enabling hardware encoding")?;
    unsafe { attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4) }
        .context("choosing the fragmented MP4 container")?;
    let url = HSTRING::from(path.as_os_str());
    unsafe { MFCreateSinkWriterFromURL(&url, &file.0, &attributes) }.context("creating the MP4 writer")
}

fn accepted_input(
    writer: &IMFSinkWriter,
    stream: u32,
    size: Size,
    fps: u32,
    inputs: &[InputPixels],
) -> Result<InputPixels> {
    let mut last_error = None;
    for &input in inputs {
        let media_type = match input {
            InputPixels::Rgb32 => rgb32_input(size, fps)?,
            InputPixels::Nv12 => nv12_input(size, fps)?,
        };
        match unsafe { writer.SetInputMediaType(stream, &media_type, None::<&IMFAttributes>) } {
            Ok(()) => return Ok(input),
            Err(error) => last_error = Some(error),
        }
    }
    let reason = last_error.map_or_else(|| "no input format was offered".to_string(), |error| error.to_string());
    Err(Error::unsupported(format!("the H.264 encoder accepts neither RGB32 nor NV12 frames: {reason}")))
}

struct LockedBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    data: *mut u8,
    length: usize,
}

impl<'a> LockedBuffer<'a> {
    fn lock(buffer: &'a IMFMediaBuffer, length: usize) -> Result<Self> {
        let mut data = std::ptr::null_mut();
        let mut capacity = 0u32;
        unsafe { buffer.Lock(&raw mut data, Some(&raw mut capacity), None) }.context("locking a video frame")?;
        let locked = Self { buffer, data, length };
        if data.is_null() || (capacity as usize) < length {
            return Err(Error::internal("Media Foundation returned a video frame buffer that is too small"));
        }
        Ok(locked)
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.data, self.length) }
    }
}

impl Drop for LockedBuffer<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.buffer.Unlock() };
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
