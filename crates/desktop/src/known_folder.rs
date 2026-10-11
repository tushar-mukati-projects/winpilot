use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use deckhand_core::{Context, Result};
use windows::Win32::{
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
};
use windows::core::PWSTR;

struct TaskMemoryText(PWSTR);

impl TaskMemoryText {
    fn to_path(&self) -> PathBuf {
        PathBuf::from(OsString::from_wide(unsafe { self.0.as_wide() }))
    }
}

impl Drop for TaskMemoryText {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast())) };
    }
}

pub fn videos_folder() -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Videos, KF_FLAG_DEFAULT, None) }
        .context("finding the Videos folder")?;
    Ok(TaskMemoryText(path).to_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_videos_folder_is_an_absolute_path() {
        let folder = videos_folder().unwrap();
        assert!(folder.is_absolute(), "{}", folder.display());
    }
}
