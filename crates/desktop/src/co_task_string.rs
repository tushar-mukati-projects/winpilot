use std::ffi::c_void;

use windows::{Win32::System::Com::CoTaskMemFree, core::PWSTR};

pub(crate) struct CoTaskString(pub(crate) PWSTR);

impl CoTaskString {
    pub(crate) fn read(&self) -> Option<String> {
        if self.0.is_null() {
            return None;
        }
        unsafe { self.0.to_string() }.ok()
    }
}

impl Drop for CoTaskString {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast::<c_void>())) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_string_reads_as_nothing() {
        assert_eq!(CoTaskString(PWSTR::null()).read(), None);
    }
}
