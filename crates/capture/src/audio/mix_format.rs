use deckhand_core::{Error, Result};
use windows::Win32::Media::Audio::{IAudioClient, WAVEFORMATEX, WAVEFORMATEXTENSIBLE};
use windows::Win32::System::Com::CoTaskMemFree;

use super::device::{Endpoint, audio_error};
use super::pcm::AudioFormat;
use super::sample_format::{SampleFormat, tag_of_subformat};

const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const EXTENSIBLE_EXTRA_BYTES: u16 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceFormat {
    pub(crate) audio: AudioFormat,
    pub(crate) sample: SampleFormat,
    pub(crate) block_align: usize,
}

pub(crate) struct MixFormat(*mut WAVEFORMATEX);

impl MixFormat {
    pub(crate) fn of(client: &IAudioClient, endpoint: Endpoint) -> Result<Self> {
        let format = unsafe { client.GetMixFormat() }.map_err(|error| audio_error(endpoint, &error, "reading"))?;
        if format.is_null() {
            return Err(Error::internal("Windows returned no audio mix format"));
        }
        Ok(Self(format))
    }

    pub(crate) const fn as_ptr(&self) -> *const WAVEFORMATEX {
        self.0
    }

    pub(crate) fn describe(&self) -> Result<DeviceFormat> {
        let header = self.header();
        let tag = self.tag(&header);
        let bits = header.wBitsPerSample;
        let sample = SampleFormat::identify(tag, bits).ok_or_else(|| {
            Error::unsupported(format!("the audio device uses an unsupported format ({tag}, {bits} bit)"))
        })?;
        describe_header(&header, sample)
    }

    fn header(&self) -> WAVEFORMATEX {
        unsafe { self.0.read_unaligned() }
    }

    fn tag(&self, header: &WAVEFORMATEX) -> u32 {
        if header.wFormatTag != WAVE_FORMAT_EXTENSIBLE || header.cbSize < EXTENSIBLE_EXTRA_BYTES {
            return u32::from(header.wFormatTag);
        }
        let extensible = unsafe { self.0.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
        tag_of_subformat(extensible.SubFormat).unwrap_or(0)
    }
}

fn describe_header(header: &WAVEFORMATEX, sample: SampleFormat) -> Result<DeviceFormat> {
    let channels = header.nChannels;
    let block_align = usize::from(header.nBlockAlign);
    if channels == 0 || header.nSamplesPerSec == 0 || block_align != usize::from(channels) * sample.bytes() {
        return Err(Error::unsupported("the audio device reports an inconsistent format"));
    }
    Ok(DeviceFormat { audio: AudioFormat { sample_rate: header.nSamplesPerSec, channels }, sample, block_align })
}

impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast_const().cast())) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(channels: u16, rate: u32, bits: u16, block_align: u16) -> WAVEFORMATEX {
        WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(block_align),
            nBlockAlign: block_align,
            wBitsPerSample: bits,
            cbSize: 0,
        }
    }

    #[test]
    fn a_consistent_header_describes_the_device_format() {
        let format = describe_header(&header(2, 48_000, 32, 8), SampleFormat::F32).unwrap();
        assert_eq!(format.audio, AudioFormat { sample_rate: 48_000, channels: 2 });
        assert_eq!((format.sample, format.block_align), (SampleFormat::F32, 8));
    }

    #[test]
    fn inconsistent_headers_are_refused() {
        assert!(describe_header(&header(0, 48_000, 32, 0), SampleFormat::F32).is_err());
        assert!(describe_header(&header(2, 0, 32, 8), SampleFormat::F32).is_err());
        assert!(describe_header(&header(2, 48_000, 32, 6), SampleFormat::F32).is_err());
    }
}
