use std::path::Path;

use deckhand_core::{Context, Error, Result, protocol::Quality};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFByteStream, IMFMediaBuffer, IMFSample, IMFSinkWriter, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_TRANSCODE_CONTAINERTYPE, MFCreateAttributes, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFTranscodeContainerType_MPEG4,
};
use windows_core::HSTRING;

use super::bitrate::average_bitrate;
use super::clock::{sample_duration, sample_time};
use super::layout::Size;
use super::media_foundation::MediaFoundation;
use super::media_type::{h264_output, nv12_input, rgb32_input};
use super::nv12::{bgra_to_nv12, nv12_len};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputPixels {
    Rgb32,
    Nv12,
}

pub(crate) const PREFERRED_INPUTS: [InputPixels; 2] = [InputPixels::Rgb32, InputPixels::Nv12];

pub(crate) struct Mp4Writer {
    writer: IMFSinkWriter,
    stream: u32,
    size: Size,
    fps: u32,
    input: InputPixels,
    _media_foundation: MediaFoundation,
}

impl Mp4Writer {
    pub(crate) fn create(path: &Path, size: Size, fps: u32, quality: Quality, inputs: &[InputPixels]) -> Result<Self> {
        let media_foundation = MediaFoundation::start()?;
        let writer = sink_writer(path)?;
        let output = h264_output(size, fps, average_bitrate(size.width, size.height, fps, quality))?;
        let stream = unsafe { writer.AddStream(&output) }
            .map_err(|error| Error::unsupported(format!("no H.264 encoder is available: {error}")))?;
        let input = accepted_input(&writer, stream, size, fps, inputs)?;
        unsafe { writer.BeginWriting() }
            .map_err(|error| Error::unsupported(format!("the H.264 encoder would not start: {error}")))?;
        Ok(Self { writer, stream, size, fps, input, _media_foundation: media_foundation })
    }

    pub(crate) fn write(&mut self, bgra: &[u8], tick: u64) -> Result<()> {
        let sample = self.sample_of(bgra)?;
        unsafe { sample.SetSampleTime(sample_time(tick, self.fps)) }.context("timing a video frame")?;
        unsafe { sample.SetSampleDuration(sample_duration(tick, self.fps)) }.context("timing a video frame")?;
        unsafe { self.writer.WriteSample(self.stream, &sample) }.context("encoding a video frame")
    }

    pub(crate) fn finish(self) -> Result<()> {
        unsafe { self.writer.Finalize() }.context("finalizing the MP4 file")
    }

    fn sample_of(&self, bgra: &[u8]) -> Result<IMFSample> {
        let length = match self.input {
            InputPixels::Rgb32 => bgra.len(),
            InputPixels::Nv12 => nv12_len(self.size),
        };
        let byte_count = u32::try_from(length).map_err(|_| Error::internal("the video frame is too large"))?;
        let buffer = unsafe { MFCreateMemoryBuffer(byte_count) }.context("allocating a video frame")?;
        self.fill(&buffer, bgra, length)?;
        unsafe { buffer.SetCurrentLength(byte_count) }.context("sizing a video frame")?;
        let sample = unsafe { MFCreateSample() }.context("creating a video sample")?;
        unsafe { sample.AddBuffer(&buffer) }.context("filling a video sample")?;
        Ok(sample)
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

fn sink_writer(path: &Path) -> Result<IMFSinkWriter> {
    let mut attributes: Option<IMFAttributes> = None;
    unsafe { MFCreateAttributes(&raw mut attributes, 2) }.context("creating the MP4 writer settings")?;
    let attributes = attributes.ok_or_else(|| Error::internal("Media Foundation returned no attribute store"))?;
    unsafe { attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1) }
        .context("enabling hardware encoding")?;
    unsafe { attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4) }
        .context("choosing the MP4 container")?;
    let url = HSTRING::from(path.as_os_str());
    unsafe { MFCreateSinkWriterFromURL(&url, None::<&IMFByteStream>, &attributes) }.context("creating the MP4 file")
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
