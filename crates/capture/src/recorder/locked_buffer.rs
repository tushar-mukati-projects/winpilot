use windows::Win32::Media::MediaFoundation::IMFMediaBuffer;
use winpilot_core::{Context, Error, Result};

pub(crate) struct LockedBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    data: *mut u8,
    length: usize,
}

impl<'a> LockedBuffer<'a> {
    pub(crate) fn lock(buffer: &'a IMFMediaBuffer, length: usize) -> Result<Self> {
        let mut data = std::ptr::null_mut();
        let mut capacity = 0u32;
        unsafe { buffer.Lock(&raw mut data, Some(&raw mut capacity), None) }.context("locking a media buffer")?;
        let locked = Self { buffer, data, length };
        if data.is_null() || (capacity as usize) < length {
            return Err(Error::internal("Media Foundation returned a media buffer that is too small"));
        }
        Ok(locked)
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.data, self.length) }
    }
}

impl Drop for LockedBuffer<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.buffer.Unlock() };
    }
}
