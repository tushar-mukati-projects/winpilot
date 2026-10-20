use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE, MF_MT_ALL_SAMPLES_INDEPENDENT,
    MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT,
    MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MFAudioFormat_AAC,
    MFAudioFormat_PCM, MFMediaType_Audio,
};
use windows::core::GUID;
use winpilot_core::Result;
use winpilot_win32::{new_media_type, set_attribute_guid, set_attribute_u32};

use crate::audio::AudioFormat;

const PCM_BITS: u32 = 16;
const PCM_BYTES: u32 = 2;
const AAC_BYTES_PER_SECOND: u32 = 20_000;
const AAC_LOW_COMPLEXITY_LEVEL_2: u32 = 0x29;
const AAC_RAW_PAYLOAD: u32 = 0;

pub(crate) fn aac_output(format: AudioFormat) -> Result<IMFMediaType> {
    let media_type = audio_type(&MFAudioFormat_AAC, format)?;
    set_attribute_u32(&media_type, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AAC_BYTES_PER_SECOND)?;
    set_attribute_u32(&media_type, &MF_MT_AAC_PAYLOAD_TYPE, AAC_RAW_PAYLOAD)?;
    set_attribute_u32(&media_type, &MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, AAC_LOW_COMPLEXITY_LEVEL_2)?;
    Ok(media_type)
}

pub(crate) fn pcm_input(format: AudioFormat) -> Result<IMFMediaType> {
    let media_type = audio_type(&MFAudioFormat_PCM, format)?;
    let block = pcm_block_align(format);
    set_attribute_u32(&media_type, &MF_MT_AUDIO_BLOCK_ALIGNMENT, block)?;
    set_attribute_u32(&media_type, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * format.sample_rate)?;
    set_attribute_u32(&media_type, &MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
    Ok(media_type)
}

const fn pcm_block_align(format: AudioFormat) -> u32 {
    PCM_BYTES * format.channels as u32
}

fn audio_type(subtype: &GUID, format: AudioFormat) -> Result<IMFMediaType> {
    let media_type = new_media_type()?;
    set_attribute_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    set_attribute_guid(&media_type, &MF_MT_SUBTYPE, subtype)?;
    set_attribute_u32(&media_type, &MF_MT_AUDIO_BITS_PER_SAMPLE, PCM_BITS)?;
    set_attribute_u32(&media_type, &MF_MT_AUDIO_SAMPLES_PER_SECOND, format.sample_rate)?;
    set_attribute_u32(&media_type, &MF_MT_AUDIO_NUM_CHANNELS, u32::from(format.channels))?;
    Ok(media_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_pcm_frames_take_four_bytes() {
        assert_eq!(pcm_block_align(AudioFormat { sample_rate: 48_000, channels: 2 }), 4);
        assert_eq!(pcm_block_align(AudioFormat { sample_rate: 44_100, channels: 1 }), 2);
    }

    #[test]
    fn the_aac_bitrate_is_160_kbps() {
        assert_eq!(AAC_BYTES_PER_SECOND * 8, 160_000);
    }
}
