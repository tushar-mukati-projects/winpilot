use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows::Win32::Foundation::HANDLE;

pub(crate) fn adopt(handle: HANDLE) -> Option<OwnedHandle> {
    if handle.is_invalid() {
        return None;
    }
    Some(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

pub(crate) fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
