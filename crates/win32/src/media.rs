use std::marker::PhantomData;

use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaBuffer, IMFMediaType, MF_VERSION, MFCreateMediaType, MFSTARTUP_NOSOCKET, MFShutdown,
    MFStartup,
};
use windows::core::GUID;
use winpilot_core::{Context, Error, Result};

pub struct MediaFoundation {
    _bound_to_thread: PhantomData<*const ()>,
}

impl MediaFoundation {
    pub fn start() -> Result<Self> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }
            .map_err(|error| Error::unsupported(format!("Media Foundation is not available: {error}")))?;
        Ok(Self { _bound_to_thread: PhantomData })
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

pub struct LockedMediaBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    data: *mut u8,
    length: usize,
}

impl<'a> LockedMediaBuffer<'a> {
    pub fn lock(buffer: &'a IMFMediaBuffer) -> Result<Self> {
        let (data, _capacity, length) = lock_buffer(buffer)?;
        Ok(Self { buffer, data, length })
    }

    pub fn lock_to_fill(buffer: &'a IMFMediaBuffer, length: usize) -> Result<Self> {
        let (data, capacity, _current) = lock_buffer(buffer)?;
        let locked = Self { buffer, data, length };
        if data.is_null() || capacity < length {
            return Err(Error::internal("Media Foundation returned a media buffer that is too small"));
        }
        Ok(locked)
    }

    pub fn bytes(&self) -> &[u8] {
        if self.data.is_null() || self.length == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.data, self.length) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        if self.data.is_null() || self.length == 0 {
            return &mut [];
        }
        unsafe { std::slice::from_raw_parts_mut(self.data, self.length) }
    }
}

impl Drop for LockedMediaBuffer<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.buffer.Unlock() };
    }
}

pub fn new_media_type() -> Result<IMFMediaType> {
    unsafe { MFCreateMediaType() }.context("creating a Media Foundation media type")
}

pub fn set_attribute_guid(attributes: &IMFAttributes, key: &GUID, value: &GUID) -> Result<()> {
    unsafe { attributes.SetGUID(key, value) }.context("setting a Media Foundation attribute")
}

pub fn set_attribute_u32(attributes: &IMFAttributes, key: &GUID, value: u32) -> Result<()> {
    unsafe { attributes.SetUINT32(key, value) }.context("setting a Media Foundation attribute")
}

pub fn set_attribute_u64(attributes: &IMFAttributes, key: &GUID, value: u64) -> Result<()> {
    unsafe { attributes.SetUINT64(key, value) }.context("setting a Media Foundation attribute")
}

pub fn attribute_u32(attributes: &IMFAttributes, key: &GUID) -> Result<u32> {
    unsafe { attributes.GetUINT32(key) }.context("reading a Media Foundation attribute")
}

fn lock_buffer(buffer: &IMFMediaBuffer) -> Result<(*mut u8, usize, usize)> {
    let mut data = std::ptr::null_mut();
    let mut capacity = 0u32;
    let mut length = 0u32;
    unsafe { buffer.Lock(&raw mut data, Some(&raw mut capacity), Some(&raw mut length)) }
        .context("locking a media buffer")?;
    Ok((data, capacity as usize, length as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComApartment;

    #[test]
    fn apartment_and_media_foundation_nest() {
        std::thread::spawn(|| {
            let outer = ComApartment::enter_any().unwrap();
            let inner = ComApartment::enter_any().unwrap();
            let media = MediaFoundation::start().unwrap();
            drop(media);
            drop(inner);
            drop(outer);
        })
        .join()
        .unwrap();
    }
}
