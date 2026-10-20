use std::{mem::size_of, os::windows::io::OwnedHandle, ptr};

use windows::Win32::{
    Foundation::HANDLE,
    Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsValidSid, TOKEN_ELEVATION,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenElevation, TokenIntegrityLevel,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
use winpilot_win32::{adopt, raw};

const LABEL_BUFFER_WORDS: usize = 32;

pub fn own_process_elevated() -> bool {
    let Some(token) = open_token(unsafe { GetCurrentProcess() }) else { return false };
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    let read = unsafe {
        GetTokenInformation(
            raw(&token),
            TokenElevation,
            Some(ptr::from_mut(&mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &raw mut returned,
        )
    };
    read.is_ok() && elevation.TokenIsElevated != 0
}

pub(crate) fn own_integrity_level() -> Option<u32> {
    process_integrity_level(unsafe { GetCurrentProcess() })
}

pub(crate) fn process_integrity_level(process: HANDLE) -> Option<u32> {
    let token = open_token(process)?;
    integrity_of_token(&token)
}

fn open_token(process: HANDLE) -> Option<OwnedHandle> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.ok()?;
    unsafe { adopt(token) }
}

fn integrity_of_token(token: &OwnedHandle) -> Option<u32> {
    let mut buffer = [0u64; LABEL_BUFFER_WORDS];
    let mut returned = 0u32;
    unsafe {
        GetTokenInformation(
            raw(token),
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            size_of::<[u64; LABEL_BUFFER_WORDS]>() as u32,
            &raw mut returned,
        )
    }
    .ok()?;
    let label = unsafe { &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() };
    last_sub_authority(label)
}

fn last_sub_authority(label: &TOKEN_MANDATORY_LABEL) -> Option<u32> {
    let sid = label.Label.Sid;
    if sid.is_invalid() || !unsafe { IsValidSid(sid) }.as_bool() {
        return None;
    }
    let count = unsafe { GetSidSubAuthorityCount(sid) };
    if count.is_null() {
        return None;
    }
    let last_index = u32::from(unsafe { *count }).checked_sub(1)?;
    let authority = unsafe { GetSidSubAuthority(sid, last_index) };
    if authority.is_null() { None } else { Some(unsafe { *authority }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEDIUM_INTEGRITY: u32 = 0x2000;

    #[test]
    fn own_integrity_level_is_at_least_medium() {
        let level = own_integrity_level().unwrap();
        assert!(level >= MEDIUM_INTEGRITY);
    }

    #[test]
    fn own_process_elevated_does_not_panic() {
        let _elevated = own_process_elevated();
    }
}
