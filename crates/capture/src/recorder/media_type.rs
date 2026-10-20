use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION,
    MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX, MFMediaType_Video, MFNominalRange_16_235,
    MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoFormat_RGB32, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709,
    MFVideoTransFunc_709, MFVideoTransferMatrix_BT709, eAVEncH264VProfile_High,
};
use windows::core::GUID;
use winpilot_core::Result;
use winpilot_win32::{new_media_type, set_attribute_guid, set_attribute_u32, set_attribute_u64};

use super::layout::Size;
use crate::pixels::BYTES_PER_PIXEL;

pub(crate) fn h264_output(size: Size, fps: u32, bitrate: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_H264, size, fps)?;
    set_attribute_u32(&media_type, &MF_MT_AVG_BITRATE, bitrate)?;
    set_attribute_u32(&media_type, &MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
    describe_bt709_limited(&media_type)?;
    Ok(media_type)
}

pub(crate) fn rgb32_input(size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_RGB32, size, fps)?;
    set_attribute_u32(&media_type, &MF_MT_DEFAULT_STRIDE, size.width * BYTES_PER_PIXEL as u32)?;
    set_attribute_u32(&media_type, &MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    set_attribute_u32(&media_type, &MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    set_attribute_u32(&media_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    Ok(media_type)
}

pub(crate) fn nv12_input(size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = video_type(&MFVideoFormat_NV12, size, fps)?;
    set_attribute_u32(&media_type, &MF_MT_DEFAULT_STRIDE, size.width)?;
    describe_bt709_limited(&media_type)?;
    Ok(media_type)
}

fn describe_bt709_limited(media_type: &IMFMediaType) -> Result<()> {
    set_attribute_u32(media_type, &MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    set_attribute_u32(media_type, &MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    set_attribute_u32(media_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    set_attribute_u32(media_type, &MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
}

fn video_type(subtype: &GUID, size: Size, fps: u32) -> Result<IMFMediaType> {
    let media_type = new_media_type()?;
    set_attribute_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    set_attribute_guid(&media_type, &MF_MT_SUBTYPE, subtype)?;
    set_attribute_u64(&media_type, &MF_MT_FRAME_SIZE, packed(size.width, size.height))?;
    set_attribute_u64(&media_type, &MF_MT_FRAME_RATE, packed(fps, 1))?;
    set_attribute_u64(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, packed(1, 1))?;
    set_attribute_u32(&media_type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    Ok(media_type)
}

const fn packed(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
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
