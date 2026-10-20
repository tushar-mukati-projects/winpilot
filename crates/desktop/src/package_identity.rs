use windows::{
    Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        Storage::Packaging::Appx::GetPackageFamilyName,
    },
    core::PWSTR,
};
use winpilot_win32::raw;

use crate::process::open_for_query;

pub fn package_family(pid: u32) -> Option<String> {
    let process = open_for_query(pid)?;
    let mut length = 0u32;
    let sized = unsafe { GetPackageFamilyName(raw(&process), &raw mut length, None) };
    if sized != ERROR_INSUFFICIENT_BUFFER || length == 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize];
    let read = unsafe { GetPackageFamilyName(raw(&process), &raw mut length, Some(PWSTR(buffer.as_mut_ptr()))) };
    if read != ERROR_SUCCESS {
        return None;
    }
    let text = buffer.split(|unit| *unit == 0).next().unwrap_or_default();
    Some(String::from_utf16_lossy(text)).filter(|family| !family.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpackaged_and_missing_processes_have_no_family() {
        assert_eq!(package_family(std::process::id()), None);
        assert_eq!(package_family(0), None);
    }
}
