use deckhand_core::{Context, Result};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION,
    MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX, MFCreateMediaType, MFMediaType_Video,
    MFNominalRange_16_235, MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoFormat_RGB32, MFVideoInterlace_Progressive,
    MFVideoPrimaries_BT709, MFVideoTransFunc_709, MFVideoTransferMatrix_BT709, eAVEncH264VProfile_High,
};
use windows_core::GUID;

use super::layout::Size;

const BYTES_PER_PIXEL: u32 = 4;

pub(crate) fn h264_output(size: Size, fps: u32, bitrate: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_H264, size, fps)?;
    set_u32(&media_type, &MF_MT_AVG_BITRATE, bitrate)?;
    set_u32(&media_type, &MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
    describe_bt709_limited(&media_type)?;
    Ok(media_type)
}

pub(crate) fn rgb32_input(size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_RGB32, size, fps)?;
    set_u32(&media_type, &MF_MT_DEFAULT_STRIDE, size.width * BYTES_PER_PIXEL)?;
    set_u32(&media_type, &MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    set_u32(&media_type, &MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    set_u32(&media_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    Ok(media_type)
}

pub(crate) fn nv12_input(size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_NV12, size, fps)?;
    set_u32(&media_type, &MF_MT_DEFAULT_STRIDE, size.width)?;
    describe_bt709_limited(&media_type)?;
    Ok(media_type)
}

fn describe_bt709_limited(media_type: &IMFMediaType) -> Result<()> {
    set_u32(media_type, &MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    set_u32(media_type, &MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    set_u32(media_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    set_u32(media_type, &MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
}

fn video_type(subtype: &GUID, size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = unsafe { MFCreateMediaType() }.context("creating a Media Foundation media type")?;
    set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    set_guid(&media_type, &MF_MT_SUBTYPE, subtype)?;
    set_u64(&media_type, &MF_MT_FRAME_SIZE, packed(size.width, size.height))?;
    set_u64(&media_type, &MF_MT_FRAME_RATE, packed(fps, 1))?;
    set_u64(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, packed(1, 1))?;
    set_u32(&media_type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    Ok(media_type)
}

pub(crate) const fn packed(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

fn set_guid(media_type: &IMFMediaType, key: &GUID, value: &GUID) -> Result<()> {
    unsafe { media_type.SetGUID(key, value) }.context("describing the video stream")
}

fn set_u32(media_type: &IMFMediaType, key: &GUID, value: u32) -> Result<()> {
    unsafe { media_type.SetUINT32(key, value) }.context("describing the video stream")
}

fn set_u64(media_type: &IMFMediaType, key: &GUID, value: u64) -> Result<()> {
    unsafe { media_type.SetUINT64(key, value) }.context("describing the video stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_rates_pack_high_then_low() {
        assert_eq!(packed(1920, 1200), 0x0780_0000_04B0);
        assert_eq!(packed(30, 1), 0x1E_0000_0001);
    }
}
