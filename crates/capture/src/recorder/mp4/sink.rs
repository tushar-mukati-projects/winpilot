use std::path::Path;

use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFByteStream, IMFSinkWriter, MF_ACCESSMODE_READWRITE, MF_FILEFLAGS_NONE,
    MF_OPENMODE_FAIL_IF_NOT_EXIST, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_TRANSCODE_CONTAINERTYPE,
    MFCreateAttributes, MFCreateFile, MFCreateSinkWriterFromURL, MFTranscodeContainerType_FMPEG4,
};
use windows::core::HSTRING;
use winpilot_core::{Context, Error, Result, protocol::Quality};
use winpilot_win32::{set_attribute_guid, set_attribute_u32};

use super::{AudioStream, InputPixels};
use crate::recorder::bitrate::average_bitrate;
use crate::recorder::layout::Size;
use crate::recorder::media_type::{h264_output, nv12_input, rgb32_input};
use crate::recorder::mp4_audio::add_audio_stream;

const WRITER_SETTING_COUNT: u32 = 2;
const ENABLED: u32 = 1;

pub(super) struct ByteStream(IMFByteStream);

#[derive(Clone, Copy)]
pub(super) struct VideoSpec<'a> {
    pub path: &'a Path,
    pub size: Size,
    pub fps: u32,
    pub quality: Quality,
    pub inputs: &'a [InputPixels],
}

pub(super) struct Sink {
    pub writer: IMFSinkWriter,
    pub file: ByteStream,
    pub stream: u32,
    pub input: InputPixels,
    pub audio: AudioStream,
}

pub(super) fn open_sink(spec: VideoSpec<'_>, with_audio: bool) -> Result<Sink> {
    let VideoSpec { path, size, fps, quality, inputs } = spec;
    let file = ByteStream::open_existing(path)?;
    let writer = sink_writer(path, &file)?;
    let output = h264_output(size, fps, average_bitrate(size.width, size.height, fps, quality))?;
    let stream = unsafe { writer.AddStream(&output) }
        .map_err(|error| Error::unsupported(format!("no H.264 encoder is available: {error}")))?;
    let input = accepted_input(&writer, stream, size, fps, inputs)?;
    let audio = audio_stream(&writer, with_audio);
    Ok(Sink { writer, file, stream, input, audio })
}

pub(super) fn without_refused_audio(sink: Sink, spec: VideoSpec<'_>) -> Result<Sink> {
    let AudioStream::Refused(reason) = &sink.audio else { return Ok(sink) };
    let audio = AudioStream::Refused(reason.clone());
    drop(sink);
    Ok(Sink { audio, ..open_sink(spec, false)? })
}

fn audio_stream(writer: &IMFSinkWriter, with_audio: bool) -> AudioStream {
    if !with_audio {
        return AudioStream::None;
    }
    match add_audio_stream(writer) {
        Ok(stream) => AudioStream::Ready(stream),
        Err(error) => AudioStream::Refused(error.message),
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
    let settings = writer_settings()?;
    let url = HSTRING::from(path.as_os_str());
    unsafe { MFCreateSinkWriterFromURL(&url, &file.0, &settings) }.context("creating the MP4 writer")
}

fn writer_settings() -> Result<IMFAttributes> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&raw mut attributes, WRITER_SETTING_COUNT) }
        .context("creating the MP4 writer settings")?;
    let attributes = attributes.ok_or_else(|| Error::internal("Media Foundation returned no attribute store"))?;
    set_attribute_u32(&attributes, &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, ENABLED)?;
    set_attribute_guid(&attributes, &MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
    Ok(attributes)
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
