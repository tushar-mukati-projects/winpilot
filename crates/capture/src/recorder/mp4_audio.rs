use windows::Win32::Media::MediaFoundation::{IMFAttributes, IMFSinkWriter};
use winpilot_core::{Error, Result};

use super::audio_backlog::TrackSamples;
use super::audio_feed::TRACK_FORMAT;
use super::audio_type::{aac_output, pcm_input};
use super::clock::sample_time;
use super::media_sample::{filled_buffer, write_sample};

const PCM_SAMPLE_BYTES: usize = size_of::<i16>();
const HZ_PER_KHZ: u32 = 1_000;

pub(crate) fn add_audio_stream(writer: &IMFSinkWriter) -> Result<u32> {
    let output = aac_output(TRACK_FORMAT)?;
    let stream = unsafe { writer.AddStream(&output) }
        .map_err(|error| Error::unsupported(format!("no AAC audio encoder is available: {error}")))?;
    let input = pcm_input(TRACK_FORMAT)?;
    unsafe { writer.SetInputMediaType(stream, &input, None::<&IMFAttributes>) }.map_err(|error| {
        let kilohertz = TRACK_FORMAT.sample_rate / HZ_PER_KHZ;
        Error::unsupported(format!("the AAC audio encoder refused {kilohertz} kHz stereo PCM: {error}"))
    })?;
    Ok(stream)
}

pub(crate) fn write_audio(writer: &IMFSinkWriter, stream: u32, samples: &TrackSamples) -> Result<()> {
    let buffer = filled_buffer(size_of_val(samples.pcm.as_slice()), |bytes| {
        for (bytes, sample) in bytes.as_chunks_mut::<PCM_SAMPLE_BYTES>().0.iter_mut().zip(&samples.pcm) {
            *bytes = sample.to_le_bytes();
        }
        Ok(())
    })?;
    let frames = (samples.pcm.len() / usize::from(TRACK_FORMAT.channels)) as u64;
    let start = sample_time(samples.first_frame, TRACK_FORMAT.sample_rate);
    let end = sample_time(samples.first_frame + frames, TRACK_FORMAT.sample_rate);
    write_sample(writer, stream, &buffer, start, end)
}
