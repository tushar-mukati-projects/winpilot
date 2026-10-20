use windows::Win32::Media::MediaFoundation::{IMFMediaBuffer, IMFSinkWriter, MFCreateMemoryBuffer, MFCreateSample};
use winpilot_core::{Context, Error, Result};
use winpilot_win32::LockedMediaBuffer;

pub(crate) fn filled_buffer(length: usize, fill: impl FnOnce(&mut [u8]) -> Result<()>) -> Result<IMFMediaBuffer> {
    let byte_count = u32::try_from(length).map_err(|_| Error::internal("the media sample is too large"))?;
    let buffer = unsafe { MFCreateMemoryBuffer(byte_count) }.context("allocating a media sample")?;
    fill(LockedMediaBuffer::lock_to_fill(&buffer, length)?.bytes_mut())?;
    unsafe { buffer.SetCurrentLength(byte_count) }.context("sizing a media sample")?;
    Ok(buffer)
}

pub(crate) fn write_sample(
    writer: &IMFSinkWriter,
    stream: u32,
    buffer: &IMFMediaBuffer,
    start: i64,
    end: i64,
) -> Result<()> {
    let sample = unsafe { MFCreateSample() }.context("creating a media sample")?;
    unsafe { sample.AddBuffer(buffer) }.context("filling a media sample")?;
    unsafe { sample.SetSampleTime(start) }.context("timing a media sample")?;
    unsafe { sample.SetSampleDuration(end - start) }.context("timing a media sample")?;
    unsafe { writer.WriteSample(stream, &sample) }.context("encoding a media sample")
}
