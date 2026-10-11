use std::{ffi::c_void, mem::ManuallyDrop};

use deckhand_core::{Context, Error, Result};
use windows::Win32::{
    Foundation::{GlobalFree, HGLOBAL},
    System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
};

pub(crate) struct OwnedGlobal(HGLOBAL);

impl OwnedGlobal {
    pub(crate) fn moveable(bytes: usize) -> Result<Self> {
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }.context("allocating clipboard memory")?;
        Ok(Self(memory))
    }

    pub(crate) fn with_bytes(bytes: &[u8]) -> Result<Self> {
        let memory = Self::moveable(bytes.len())?;
        let mut locked = LockedGlobal::lock(memory.handle())?;
        let Some(destination) = locked.bytes_mut().get_mut(..bytes.len()) else {
            return Err(Error::internal("clipboard memory is smaller than requested"));
        };
        destination.copy_from_slice(bytes);
        drop(locked);
        Ok(memory)
    }

    pub(crate) fn handle(&self) -> HGLOBAL {
        self.0
    }

    pub(crate) fn into_raw(self) -> HGLOBAL {
        ManuallyDrop::new(self).0
    }
}

impl Drop for OwnedGlobal {
    fn drop(&mut self) {
        let _ = unsafe { GlobalFree(Some(self.0)) };
    }
}

pub(crate) struct LockedGlobal {
    memory: HGLOBAL,
    start: *mut c_void,
    bytes: usize,
}

impl LockedGlobal {
    pub(crate) fn lock(memory: HGLOBAL) -> Result<Self> {
        let start = unsafe { GlobalLock(memory) };
        if start.is_null() {
            return Err(Error::internal(format!("locking clipboard memory: {}", windows::core::Error::from_thread())));
        }
        let bytes = unsafe { GlobalSize(memory) };
        Ok(Self { memory, start, bytes })
    }

    pub(crate) fn utf16_units(&self) -> &[u16] {
        let units = self.bytes / size_of::<u16>();
        unsafe { std::slice::from_raw_parts(self.start.cast::<u16>(), units) }
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.start.cast::<u8>(), self.bytes) }
    }
}

impl Drop for LockedGlobal {
    fn drop(&mut self) {
        let _ = unsafe { GlobalUnlock(self.memory) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_holds_the_bytes_it_was_given() {
        let memory = OwnedGlobal::with_bytes(&[1, 0, 2, 0, 0, 0]).unwrap();
        let locked = LockedGlobal::lock(memory.handle()).unwrap();
        assert_eq!(locked.utf16_units().get(..3), Some(&[1u16, 2, 0][..]));
    }
}
