use std::path::{Path, PathBuf};

use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaBuffer, IMFSample, IMFSourceReader, MF_E_INVALIDSTREAMNUMBER,
    MF_E_UNSUPPORTED_BYTESTREAM_TYPE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_FIRST_AUDIO_STREAM, MF_SOURCE_READER_FLAG,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
    MFAudioFormat_Float, MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Audio,
};
use windows::core::HSTRING;
use winpilot_core::{Context, Error, Result};

use crate::audio::{MAX_SAMPLE_RATE, MIN_SAMPLE_RATE, WHISPER_SAMPLE_RATE};
use crate::com::{ComApartment, MediaFoundation};

pub const MAX_DECODED_SAMPLES: usize = 16_000 * 60 * 60 * 2;
const AUDIO_STREAM: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioFormat {
    sample_rate: u32,
    channels: u16,
}

pub fn decode_media(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let path = existing_file(path)?;
    let _com = ComApartment::enter_mta()?;
    let _media = MediaFoundation::start()?;
    let reader = open_reader(&path)?;
    select_audio_only(&reader, &path)?;
    let format = request_float_output(&reader)?;
    let samples = read_all_samples(&reader, format, &path)?;
    Ok((samples, format.sample_rate, format.channels))
}

fn existing_file(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(Error::not_found(format!("no media file at {}", path.display())));
    }
    std::path::absolute(path).context("resolving the media path")
}

fn open_reader(path: &Path) -> Result<IMFSourceReader> {
    let url = HSTRING::from(path.as_os_str());
    unsafe { MFCreateSourceReaderFromURL(&url, None::<&IMFAttributes>) }.map_err(|error| {
        if error.code() == MF_E_UNSUPPORTED_BYTESTREAM_TYPE {
            return Error::unsupported(format!(
                "Windows cannot decode {}; its format is not supported",
                path.display()
            ));
        }
        Error::internal(format!("opening {} with Media Foundation: {error}", path.display()))
    })
}

fn select_audio_only(reader: &IMFSourceReader, path: &Path) -> Result<()> {
    unsafe { reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false) }
        .context("clearing the stream selection")?;
    unsafe { reader.SetStreamSelection(AUDIO_STREAM, true) }.map_err(|error| {
        if error.code() == MF_E_INVALIDSTREAMNUMBER {
            return Error::invalid(format!("{} has no audio track to transcribe", path.display()));
        }
        Error::internal(format!("selecting the audio track: {error}"))
    })
}

fn request_float_output(reader: &IMFSourceReader) -> Result<AudioFormat> {
    let whisper_format = AudioFormat { sample_rate: WHISPER_SAMPLE_RATE, channels: 1 };
    if set_float_output(reader, Some(whisper_format)).is_err() {
        set_float_output(reader, None)?;
    }
    current_format(reader)
}

fn set_float_output(reader: &IMFSourceReader, format: Option<AudioFormat>) -> Result<()> {
    let media_type = unsafe { MFCreateMediaType() }.context("creating an audio media type")?;
    unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio) }.context("setting the major type")?;
    unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float) }.context("setting the float subtype")?;
    if let Some(format) = format {
        unsafe { media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, format.sample_rate) }
            .context("setting the sample rate")?;
        unsafe { media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(format.channels)) }
            .context("setting the channel count")?;
    }
    unsafe { reader.SetCurrentMediaType(AUDIO_STREAM, None, &media_type) }
        .map_err(|error| Error::unsupported(format!("Windows cannot decode this audio track to float: {error}")))
}

fn current_format(reader: &IMFSourceReader) -> Result<AudioFormat> {
    let media_type = unsafe { reader.GetCurrentMediaType(AUDIO_STREAM) }.context("reading the audio format")?;
    let sample_rate =
        unsafe { media_type.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.context("reading the sample rate")?;
    let channels = unsafe { media_type.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.context("reading the channel count")?;
    let channels = u16::try_from(channels).ok().filter(|count| (1..=32).contains(count));
    match channels {
        Some(channels) if (MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) => {
            Ok(AudioFormat { sample_rate, channels })
        }
        _ => Err(Error::unsupported(format!("the audio track has an unusable format ({sample_rate} Hz)"))),
    }
}

fn read_all_samples(reader: &IMFSourceReader, format: AudioFormat, path: &Path) -> Result<Vec<f32>> {
    let mut samples = Vec::new();
    loop {
        let (flags, sample) = read_sample(reader)?;
        if has_flag(flags, MF_SOURCE_READERF_ERROR) {
            return Err(Error::internal(format!("Media Foundation failed while decoding {}", path.display())));
        }
        if has_flag(flags, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED) && current_format(reader)? != format {
            return Err(Error::unsupported(format!("the audio format changes part-way through {}", path.display())));
        }
        if let Some(sample) = sample {
            append_floats(&sample, &mut samples)?;
        }
        if samples.len() > MAX_DECODED_SAMPLES {
            return Err(Error::invalid(format!("{} is too long to transcribe; trim it first", path.display())));
        }
        if has_flag(flags, MF_SOURCE_READERF_ENDOFSTREAM) {
            return Ok(samples);
        }
    }
}

fn read_sample(reader: &IMFSourceReader) -> Result<(u32, Option<IMFSample>)> {
    let mut flags = 0_u32;
    let mut sample = None;
    unsafe { reader.ReadSample(AUDIO_STREAM, 0, None, Some(&raw mut flags), None, Some(&raw mut sample)) }
        .context("reading audio from the media file")?;
    Ok((flags, sample))
}

fn has_flag(flags: u32, flag: MF_SOURCE_READER_FLAG) -> bool {
    flags & flag.0 as u32 != 0
}

fn append_floats(sample: &IMFSample, samples: &mut Vec<f32>) -> Result<()> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.context("reading an audio buffer")?;
    let locked = LockedBuffer::lock(&buffer)?;
    samples.extend(locked.bytes().as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)));
    Ok(())
}

struct LockedBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    data: *mut u8,
    length: usize,
}

impl<'a> LockedBuffer<'a> {
    fn lock(buffer: &'a IMFMediaBuffer) -> Result<Self> {
        let mut data = std::ptr::null_mut();
        let mut length = 0_u32;
        unsafe { buffer.Lock(&raw mut data, None, Some(&raw mut length)) }.context("locking an audio buffer")?;
        Ok(Self { buffer, data, length: length as usize })
    }

    fn bytes(&self) -> &[u8] {
        if self.data.is_null() || self.length == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.data, self.length) }
    }
}

impl Drop for LockedBuffer<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.buffer.Unlock() };
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("winpilot-speech-test-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn wav_bytes(frames: &[i16], channels: u16, rate: u32) -> Vec<u8> {
        let data_len = (frames.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend(frames.iter().flat_map(|value| value.to_le_bytes()));
        bytes
    }

    fn stereo_tone(rate: u32, seconds: u32) -> Vec<i16> {
        (0..rate * seconds)
            .flat_map(|index| {
                let value = (f64::from(index) * 2.0 * std::f64::consts::PI * 440.0 / f64::from(rate)).sin();
                let sample = (value * 16_000.0) as i16;
                [sample, sample]
            })
            .collect()
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let error = decode_media(Path::new(r"Z:\winpilot-speech-test\missing.wav")).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    #[ignore = "writes a temporary wav file and decodes it with Media Foundation"]
    fn decodes_a_generated_wav() {
        let dir = TempDir::new("wav");
        let path = dir.0.join("tone.wav");
        std::fs::write(&path, wav_bytes(&stereo_tone(48_000, 2), 2, 48_000)).unwrap();
        let (samples, rate, channels) = decode_media(&path).unwrap();
        let seconds = samples.len() as f64 / f64::from(rate) / f64::from(channels);
        assert!((seconds - 2.0).abs() < 0.05, "{seconds} s at {rate} Hz x {channels}");
        let rms = (samples.iter().map(|value| f64::from(*value).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
        assert!((rms - 0.345).abs() < 0.03, "{rms}");
    }

    #[test]
    #[ignore = "writes a temporary file and asks Media Foundation to decode it"]
    fn a_file_that_is_not_media_is_refused() {
        let dir = TempDir::new("text");
        let path = dir.0.join("notes.mp3");
        std::fs::write(&path, b"this is not audio").unwrap();
        let error = decode_media(&path).unwrap_err();
        assert!(matches!(error.code, ErrorCode::Unsupported | ErrorCode::InvalidArgument), "{error}");
    }
}
