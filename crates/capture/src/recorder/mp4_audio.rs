use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaBuffer, IMFSinkWriter, MFCreateMemoryBuffer, MFCreateSample,
};
use winpilot_core::{Context, Error, Result};

use super::audio_backlog::TrackSamples;
use super::audio_feed::TRACK_FORMAT;
use super::audio_type::{aac_output, pcm_input};
use super::clock::audio_sample_time;
use super::locked_buffer::LockedBuffer;

pub(crate) fn add_audio_stream(writer: &IMFSinkWriter) -> Result<u32> {
    let output = aac_output(TRACK_FORMAT)?;
    let stream = unsafe { writer.AddStream(&output) }
        .map_err(|error| Error::unsupported(format!("no AAC audio encoder is available: {error}")))?;
    let input = pcm_input(TRACK_FORMAT)?;
    unsafe { writer.SetInputMediaType(stream, &input, None::<&IMFAttributes>) }
        .map_err(|error| Error::unsupported(format!("the AAC audio encoder refused 48 kHz stereo PCM: {error}")))?;
    Ok(stream)
}

pub(crate) fn write_audio(writer: &IMFSinkWriter, stream: u32, samples: &TrackSamples) -> Result<()> {
    let buffer = pcm_buffer(&samples.pcm)?;
    let frames = (samples.pcm.len() / usize::from(TRACK_FORMAT.channels)) as u64;
    let start = audio_sample_time(samples.first_frame, TRACK_FORMAT.sample_rate);
    let end = audio_sample_time(samples.first_frame + frames, TRACK_FORMAT.sample_rate);
    let sample = unsafe { MFCreateSample() }.context("creating an audio sample")?;
    unsafe { sample.AddBuffer(&buffer) }.context("filling an audio sample")?;
    unsafe { sample.SetSampleTime(start) }.context("timing an audio sample")?;
    unsafe { sample.SetSampleDuration(end - start) }.context("timing an audio sample")?;
    unsafe { writer.WriteSample(stream, &sample) }.context("encoding audio")
}

fn pcm_buffer(pcm: &[i16]) -> Result<IMFMediaBuffer> {
    let length = size_of_val(pcm);
    let byte_count = u32::try_from(length).map_err(|_| Error::internal("the audio sample is too large"))?;
    let buffer = unsafe { MFCreateMemoryBuffer(byte_count) }.context("allocating an audio sample")?;
    let mut locked = LockedBuffer::lock(&buffer, length)?;
    for (bytes, sample) in locked.bytes_mut().as_chunks_mut::<2>().0.iter_mut().zip(pcm) {
        *bytes = sample.to_le_bytes();
    }
    drop(locked);
    unsafe { buffer.SetCurrentLength(byte_count) }.context("sizing an audio sample")?;
    Ok(buffer)
}
